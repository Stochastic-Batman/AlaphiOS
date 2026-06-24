use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use spin::Lazy;
use crate::arch::syscall::{TrapFrame};
use crate::process::lifecycle;
use crate::process::pid::{Pid, Tid};
use crate::process::table::TaskRegistry;
use crate::scheduler;
use crate::sync::spinlock::Spinlock;
use crate::sync::resource::{LockHandle, QueuedResource};
use crate::sync::resource_table::RESOURCE_TABLE;
use crate::syscall::numbers::*;
use crate::ipc::table::IPC_TABLE;
use crate::fs;
use crate::fs::vfs::FsError;


// Standard Kernel Error Codes (Negative isize for ABI boundary)
const ENOSYS: isize = -38;
const EBADF: isize = -9;
const EFAULT: isize = -14;
const EAGAIN: isize = -11;
const EPIPE: isize = -32;
const EMSGSIZE: isize = -90;
const ENOENT: isize = -2;
const EACCES: isize = -13;
const EEXIST: isize = -17;
const ENOTDIR: isize = -20;
const EISDIR: isize = -21;
const EBUSY: isize = -16;
const EROFS: isize = -30;
const ENODEV: isize = -19;


static SHM_REGISTRY: Lazy<Spinlock<BTreeMap<alloc::string::String, Arc<crate::ipc::shm::SharedMem>>>> = Lazy::new(|| Spinlock::new(BTreeMap::new()));


fn fs_err(e: FsError) -> isize {
    match e {
        FsError::NotFound => ENOENT,
        FsError::AlreadyExists => EEXIST,
        FsError::PermissionDenied => EACCES,
        FsError::Locked => EBUSY,
        FsError::IsShared => EROFS,
        FsError::NotOpen => EBADF,
        FsError::InvalidPath => EFAULT,
        FsError::NotADirectory => ENOTDIR,
        FsError::IsADirectory => EISDIR,
        FsError::NotMounted => ENODEV,
    }
}


unsafe fn str_from_user(ptr: usize, len: usize) -> Option<&'static str> {
    if ptr == 0 || len == 0 { return None; }
    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(slice).ok()
}


// Hardware saves RIP -> RCX, RFLAGS -> R11 and swaps CS/SS.
// Syscall ABI: nr=RAX, args=RDI RSI RDX R10 R8 R9
// C call ABI:  args=RDI RSI RDX RCX R8 R9
// Only mismatch is arg3: move R10 -> RCX before the call.
// Most of the stuff below is written by with the help of Claude Sonnet 4.6
#[unsafe(no_mangle)]
pub extern "C" fn syscall_handler(nr: usize, frame: &mut TrapFrame) -> isize {
    let arg0 = frame.rdi as usize;
    let arg1 = frame.rsi as usize;
    let arg2 = frame.rdx as usize;
    let arg3 = frame.r10 as usize;
    let arg4 = frame.r8  as usize;

    match nr {
        SYS_GETPID  => scheduler::current_pid().as_u64() as isize,
        SYS_GETPPID => scheduler::current_ppid().map(|p| p.as_u64() as isize).unwrap_or(-1),
        SYS_GETTID  => scheduler::current_tid().as_u64() as isize,
        SYS_YIELD   => { 
            scheduler::schedule();
            0 
        }
        SYS_EXIT    => lifecycle::exit(arg0 as i32),
        SYS_WAITPID => lifecycle::wait(Pid::from_u64(arg0 as u64)) as isize,
        SYS_GETRESOURCE => {
            let res: Arc<dyn LockHandle> = Arc::new(QueuedResource::new());
            RESOURCE_TABLE.lock().insert(res) as isize
        }
        SYS_MUTEX_LOCK => {
            let res = RESOURCE_TABLE.lock().get(arg0 as u64);
            match res {
                Some(res) => { res.lock(); 0 }
                None => EBADF,
            }
        }
        SYS_MUTEX_TRYLOCK | SYS_TRYLOCK => {
            let res = RESOURCE_TABLE.lock().get(arg0 as u64);
            match res {
                Some(res) => if res.try_lock() { 0 } else { EAGAIN },
                None => EBADF,
            }
        }
        SYS_MUTEX_UNLOCK => {
            let res = RESOURCE_TABLE.lock().get(arg0 as u64);
            match res {
                Some(res) => { res.unlock(); 0 }
                None => EBADF,
            }
        }
        SYS_CONDVAR_WAIT => {
            let res = RESOURCE_TABLE.lock().get(arg0 as u64);
            match res {
                Some(res) => { res.wait(); 0 }
                None => EBADF,
            }
        }
        SYS_CONDVAR_SIGNAL => {
            let res = RESOURCE_TABLE.lock().get(arg0 as u64);
            match res {
                Some(res) => { res.signal(); 0 }
                None => EBADF,
            }
        }
        SYS_CONDVAR_BROADCAST => {
            let res = RESOURCE_TABLE.lock().get(arg0 as u64);
            match res {
                Some(res) => { res.broadcast(); 0 }
                None => EBADF,
            }
        }
        SYS_PIPE_CREATE => {
            let pipe = Arc::new(crate::ipc::pipe::Pipe::new());
            let handle = IPC_TABLE.lock().insert(pipe);
            handle as isize
        }
        SYS_PIPE_CLOSE => {
            let handle = arg0 as u64;
            if let Some(ch) = IPC_TABLE.lock().get(handle) {
                ch.close(); // Wake up any pending blocked threads
                IPC_TABLE.lock().remove(handle);
                0
            } else {
                EBADF
            }
        }
        SYS_PIPE_READ | SYS_MSG_RECV => {  // IPC: Poly-morphic Trait Read/Write Handlers
            let handle = arg0 as u64;
            let buf_ptr = arg1 as *mut u8;
            let len = arg2;

            if len > 0 && buf_ptr.is_null() {
                return EFAULT;
            }

            if let Some(ch) = IPC_TABLE.lock().get(handle) {
                let slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                match ch.recv(slice) {
                    Ok(bytes_read) => bytes_read as isize,
                    Err(e) => match e {
                        crate::ipc::channel::IpcError::Closed => EPIPE,
                        crate::ipc::channel::IpcError::WouldBlock => EAGAIN,
                        crate::ipc::channel::IpcError::TooLarge => EMSGSIZE,
                    }
                }
            } else {
                EBADF
            }
        }
        SYS_PIPE_WRITE | SYS_MSG_SEND => {
            let handle = arg0 as u64;
            let buf_ptr = arg1 as *const u8;
            let len = arg2;

            if len > 0 && buf_ptr.is_null() {
                return EFAULT;
            }

            if let Some(ch) = IPC_TABLE.lock().get(handle) {
                let slice = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
                match ch.send(slice) {
                    Ok(()) => 0,
                    Err(e) => match e {
                        crate::ipc::channel::IpcError::Closed => EPIPE,
                        crate::ipc::channel::IpcError::WouldBlock => EAGAIN,
                        crate::ipc::channel::IpcError::TooLarge => EMSGSIZE,
                    }
                }
            } else {
                EBADF
            }
        }
        SYS_SHM_OPEN => {  // IPC: Shared Memory Operations
            let name_ptr = arg0 as *const u8;
            let name_len = arg1;
            let size = arg2;

            if name_ptr.is_null() || name_len == 0 {
                return EFAULT;
            }

            let name_slice = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
            let name = alloc::string::String::from_utf8_lossy(name_slice).into_owned();

            let mut registry = SHM_REGISTRY.lock();
            let shm = registry.entry(name.clone()).or_insert_with(|| {
                Arc::new(crate::ipc::shm::SharedMem::new(name, size)) 
            }).clone();

            let handle = IPC_TABLE.lock().insert(shm);
            handle as isize
        }
        SYS_SHM_CLOSE => {
            let handle = arg0 as u64;
            IPC_TABLE.lock().remove(handle);
            0
        }
        SYS_OPEN => {
            let path = unsafe { str_from_user(arg0, arg1) };
            match path {
                None => EFAULT,
                Some(p) => {
                    let pid = scheduler::current_pid();
                    match fs::FS.lock().open(p, pid) {
                        Ok(fd) => fd as isize,
                        Err(e) => fs_err(e),
                    }
                }
            }
        }
        SYS_CLOSE => {
            let pid = scheduler::current_pid();
            match fs::FS.lock().close(arg0 as u64, pid) {
                Ok(()) => 0,
                Err(e) => fs_err(e),
            }
        }
        SYS_READ => {
            let buf_ptr = arg1 as *mut u8;
            let len = arg2;
            if len > 0 && buf_ptr.is_null() { return EFAULT; }
            let pid = scheduler::current_pid();
            let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
            match fs::FS.lock().read(arg0 as u64, pid, buf) {
                Ok(n)  => n as isize,
                Err(e) => fs_err(e),
            }
        }
        SYS_WRITE => {
            let buf_ptr = arg1 as *const u8;
            let len = arg2;
            if len > 0 && buf_ptr.is_null() { return EFAULT; }
            let pid = scheduler::current_pid();
            let buf = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
            match fs::FS.lock().write(arg0 as u64, pid, buf) {
                Ok(n)  => n as isize,
                Err(e) => fs_err(e),
            }
        }
        SYS_SEEK => {
            let pid = scheduler::current_pid();
            match fs::FS.lock().seek(arg0 as u64, pid, arg1 as u64) {
                Ok(()) => 0,
                Err(e) => fs_err(e),
            }
        }
        SYS_CREATE => {
            let path = unsafe { str_from_user(arg0, arg1) };
            match path {
                None => EFAULT,
                Some(p) => {
                    let pid = scheduler::current_pid();
                    match fs::FS.lock().create(p, pid) {
                        Ok(fd) => fd as isize,
                        Err(e) => fs_err(e),
                    }
                }
            }
        }
        SYS_DELETE => {
            let path = unsafe { str_from_user(arg0, arg1) };
            match path {
                None => EFAULT,
                Some(p) => match fs::FS.lock().delete(p) {
                    Ok(()) => 0,
                    Err(e) => fs_err(e),
                }
            }
        }
        SYS_RENAME => {
            let old = unsafe { str_from_user(arg0, arg1) };
            let new = unsafe { str_from_user(arg2, arg3) };
            match (old, new) {
                (Some(o), Some(n)) => match fs::FS.lock().rename(o, n) {
                    Ok(()) => 0,
                    Err(e) => fs_err(e),
                },
                _ => EFAULT,
            }
        }
        SYS_MKDIR => {
            let path = unsafe { str_from_user(arg0, arg1) };
            match path {
                None => EFAULT,
                Some(p) => match fs::FS.lock().mkdir(p) {
                    Ok(()) => 0,
                    Err(e) => fs_err(e),
                }
            }
        }
        SYS_RMDIR => {
            let path = unsafe { str_from_user(arg0, arg1) };
            match path {
                None => EFAULT,
                Some(p) => match fs::FS.lock().rmdir(p) {
                    Ok(()) => 0,
                    Err(e) => fs_err(e),
                }
            }
        }
        SYS_CHMOD => {
            let path = unsafe { str_from_user(arg0, arg1) };
            match path {
                None => EFAULT,
                Some(p) => {
                    let bits = arg2 as u8;
                    let owner = (bits >> 6) & 0b111;
                    let group = (bits >> 3) & 0b111;
                    let world =  bits       & 0b111;
                    match fs::FS.lock().chmod(p, owner, group, world) {
                        Ok(()) => 0,
                        Err(e) => fs_err(e),
                    }
                }
            }
        }
        SYS_MARK_SHARED => {
            let path = unsafe { str_from_user(arg0, arg1) };
            match path {
                None => EFAULT,
                Some(p) => match fs::FS.lock().mark_shared(p) {
                    Ok(()) => 0,
                    Err(e) => fs_err(e),
                }
            }
        }
        SYS_GETDENTS => {
            let path = unsafe { str_from_user(arg0, arg1) };
            let buf_ptr = arg2 as *mut u8;
            let buf_len = arg3;

            let Some(p) = path else { return EFAULT };
            if buf_len > 0 && buf_ptr.is_null() { return EFAULT; }

            match fs::FS.lock().getdents(p) {
                Ok(entries) => {
                    const NAME_MAX: usize = 64;
                    const RECORD_SIZE: usize = NAME_MAX + 1;

                    let capacity = buf_len / RECORD_SIZE;
                    let n = entries.len().min(capacity);

                    for (i, (name, is_dir)) in entries.iter().take(n).enumerate() {
                        let record = unsafe { buf_ptr.add(i * RECORD_SIZE) };
                        let name_bytes = name.as_bytes();
                        let copy_len = name_bytes.len().min(NAME_MAX);

                        unsafe {
                            core::ptr::copy_nonoverlapping(name_bytes.as_ptr(), record, copy_len);
                            if copy_len < NAME_MAX {
                                core::ptr::write_bytes(record.add(copy_len), 0, NAME_MAX - copy_len);
                            }
                            record.add(NAME_MAX).write(*is_dir as u8);
                        }
                    }

                    n as isize
                }
                Err(e) => fs_err(e),
            }
        }
        SYS_STAT => {
            let path = unsafe { str_from_user(arg0, arg1) };
            let out_ptr = arg2 as *mut u8;

            match path {
                None => EFAULT,
                Some(p) => {
                    if out_ptr.is_null() { return EFAULT; }
                    match fs::FS.lock().stat(p) {
                        Ok(s) => {
                            unsafe { s.write_to_user(out_ptr) };
                            0
                        }
                        Err(e) => fs_err(e),
                    }
                }
            }
        }
        SYS_LOGIN => {
            let pwd_ptr = arg1 as *const u8;
            let pwd_len = arg2;
            if pwd_ptr.is_null() || pwd_len == 0 { return EFAULT; }
            let pwd = unsafe { core::slice::from_raw_parts(pwd_ptr, pwd_len) };
            let ok = fs::FS.lock().verify_password(arg0 as u32, pwd);
            if ok { 0 } else { EACCES }
        }
        SYS_CLONE => {
            const CLONE_VM: usize = 0x100;
            if arg0 & CLONE_VM == 0 { return ENOSYS; }
            let entry: fn() -> ! = unsafe { core::mem::transmute(arg1) };
            let curr_tid = scheduler::current_tid();

            let thread = {
                let table = crate::process::table::TASK_TABLE.lock();
                match table.get(curr_tid) {
                    None => return EBADF,
                    Some(arc) => arc.lock().clone_thread(entry, arg2 as u8),
                }
            };
            
            let tid = thread.tid.as_u64();
            scheduler::spawn(thread);
            
            tid as isize
        }
        SYS_JOIN => {
            lifecycle::wait_tid(Tid::from_u64(arg0 as u64)) as isize
        }
        SYS_FORK => {
            let entry: fn() -> ! = unsafe { core::mem::transmute(arg0) };
            let curr_tid = scheduler::current_tid();
            let phys_offset = x86_64::VirtAddr::new(crate::arch::paging::PHYS_MEM_OFFSET);
            let mut parent_mapper = unsafe { crate::arch::paging::PageMapper::new(phys_offset) };

            let (child_l4, mut child_mapper) = {
                let mut fa = crate::memory::FRAME_ALLOCATOR.lock();
                parent_mapper.clone_kernel_half(&mut *fa)
            };

            let child_pid = {
                let mut fa = crate::memory::FRAME_ALLOCATOR.lock();
                let table = crate::process::table::TASK_TABLE.lock();
                
                let parent_arc = match table.get(curr_tid) {
                    Some(a) => a,
                    None => return EBADF,
                };
                
                let mut parent = parent_arc.lock();
                let child_vmm = parent.vmm.fork_cow(&mut child_mapper, &mut parent_mapper, &mut *fa);
                let mut child = parent.fork_kernel(entry, parent.priority);
                child.cr3 = child_l4.start_address().as_u64();
                child.vmm = child_vmm;
                let pid = child.pid;
                
                drop(parent); drop(table);
                scheduler::spawn(child);
            
                pid
            };

            child_pid.as_u64() as isize
        }
        _ => ENOSYS,
    }
}
