use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use spin::Lazy;
use crate::process::lifecycle;
use crate::process::pid::Pid;
use crate::scheduler;
use crate::sync::spinlock::Spinlock;
use crate::syscall::numbers::*;
use crate::ipc::table::IPC_TABLE;


// Standard Kernel Error Codes (Negative isize for ABI boundary)
const ENOSYS: isize = -38;
const EBADF: isize = -9;
const EFAULT: isize = -14;
const EAGAIN: isize = -11;
const EPIPE: isize = -32;
const EMSGSIZE: isize = -90;


static SHM_REGISTRY: Lazy<Spinlock<BTreeMap<alloc::string::String, Arc<crate::ipc::shm::SharedMem>>>> = Lazy::new(|| Spinlock::new(BTreeMap::new()));


// Hardware saves RIP -> RCX, RFLAGS -> R11 and swaps CS/SS.
// Syscall ABI: nr=RAX, args=RDI RSI RDX R10 R8 R9
// C call ABI:  args=RDI RSI RDX RCX R8 R9
// Only mismatch is arg3: move R10 -> RCX before the call.
#[unsafe(no_mangle)]
pub extern "C" fn syscall_handler(nr: usize, arg0: usize, arg1: usize, arg2: usize, _arg3: usize, _arg4: usize) -> isize {
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
        _ => ENOSYS,
    }
}
