#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
extern crate alloc;
 

use alloc::boxed::Box;
use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use bootloader_api::config::Mapping;
use core::fmt::Write;
use core::panic::PanicInfo;
use spin::Mutex;
use uart_16550::SerialPort;
use crate::arch::paging::{PageMapper, PHYS_MEM_OFFSET};
use crate::arch::syscall::TrapFrame;
use crate::fs::fat::FatFs;
use crate::io::disk::{DiskDevice, RamDisk};
use crate::memory::FRAME_ALLOCATOR;
use crate::process::lifecycle;
use crate::process::table::{TASK_TABLE, TaskRegistry};
use crate::process::task::Task;
use crate::sync::atomic::KernelAtomicUsize;
use crate::sync::monitor::Monitor;
use crate::syscall::dispatch::syscall_handler;
use crate::syscall::numbers::*;
 

mod arch;
mod fs;
mod io;
mod ipc;
mod memory;
mod process;
mod scheduler;
mod security;
mod sync;
mod syscall;


const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut cfg = BootloaderConfig::new_default();
    cfg.mappings.physical_memory = Some(Mapping::FixedAddress(0xFFFF_8000_0000_0000));
    cfg.kernel_stack_size = 64 * 4096;
    cfg
};

const BOOT_DISK_SECTOR_SIZE: usize = 512;
const BOOT_DISK_SECTOR_COUNT: usize = (64 * 1024 * 1024) / BOOT_DISK_SECTOR_SIZE;
const PING_PONG_HANDOFFS: u32 = 20;

// Problem: Kernel has no screen driver yet. The VGA framebuffer exists but I haven't written code to talk to it. I need some way to print debug output while building everything else. Serial is the answer because it requires almost no setup.
// UART (Universal Asynchronous Receiver-Transmitter) is a hardware protocol for sending bytes one bit at a time over a wire. The 16550 is a specific chip implementing it - it's been in PCs since the 1980s and every PC-compatible machine (including QEMU's emulated one) has it. "Serial port" and "COM port" are the same thing from the software side.


// The 16550 UART chip lives at I/O port 0x3F8 (COM1) on every x86 PC.
// QEMU forwards writes here to stdout when launched with `-serial stdio`.
// I wrap it in a Mutex so interrupt handlers and the main path can both
// print without corrupting each other's output.
static SERIAL: Mutex<SerialPort> = unsafe {
    // SAFETY: 0x3F8 is the standard COM1 base address; nothing else in the
    // kernel touches this port. `SerialPort::new` is a const fn that only
    // stores the address - no I/O happens until `init()` is called below.
    Mutex::new(SerialPort::new(0x3F8))
};


static CLONE_FLAG: KernelAtomicUsize = KernelAtomicUsize::new(0);
static FORK_FLAG: KernelAtomicUsize = KernelAtomicUsize::new(0);

struct PingPongState { turn: u8, count: u32 }
static PING_PONG: Monitor<PingPongState> = Monitor::new(PingPongState { turn: 0, count: 0 });


pub fn serial_init() {
    // Configures baud rate (38400), word length (8 bits), no parity, 1 stop
    // bit, and enables the FIFO buffer. Must be called before any printing.
    SERIAL.lock().init();
}


// Internal function called by the macros. Having the locking here means
// `use core::fmt::Write` only appears once instead of inside every macro arm.
pub fn _print(args: core::fmt::Arguments) {
    // write_fmt returns a fmt::Result; serial writes never fail in practice
    // (the FIFO just blocks), so we discard the result.
    SERIAL.lock().write_fmt(args).ok();
}


// Macros (I hate macros, so I asked Claude Sonnet 4.6 to generate them)
// `$crate` expands to the path of the crate that defined the macro.
// This makes the macros work correctly when called from any submodule

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => { $crate::serial_print!("\n") };
    ($($arg:tt)*) => { $crate::serial_print!("{}\n", format_args!($($arg)*)) };
}


entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);
 

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial_init();
    serial_println!("AlaphiOS booting...");
 
    arch::init();
    memory::init(boot_info);
    { let probe = Box::new(0xDEAD_BEEFu64); serial_println!("heap: {:x} @ {:p}", *probe, probe); }
    mount_boot_disk();
    load_system_domain();
    memory::swap::init();
    arch::paging::record_kernel_l4_entries();
    init_devices();
    scheduler::init();
    seed_test_user();
    install_init_binary();
 
    all_the_freaking_tests();

    scheduler::spawn(Task::new(memory::swap::reaper_main, (scheduler::mlfq::NUM_QUEUES - 1) as u8, None));
    scheduler::spawn(Task::new(ping_pong_a, 0, None));
    scheduler::spawn(Task::new(ping_pong_b, 0, None));

    serial_println!("boot complete");
    loop { x86_64::instructions::hlt(); }
}


fn all_the_freaking_tests() {
    io_smoke_test();
    sync_syscall_smoke_test();
    fs_smoke_test();
    security_smoke_test();
    device_smoke_test();
    disk_scheduling_smoke_test();
    consistency_smoke_test();
    swap_smoke_test();
    clone_smoke_test();
    fork_smoke_test();
    user_mode_smoke_test();
}


// all of this smoke tests are written by Claude Sonnet 4.6
fn mount_boot_disk() {
    let disk = RamDisk::new(BOOT_DISK_SECTOR_COUNT, BOOT_DISK_SECTOR_SIZE);
    let fatfs = FatFs::format_and_mount(disk).expect("failed to format/mount boot FAT32 volume");
    fs::FS.lock().mount_disk(fatfs);
}
 
fn load_system_domain() {
    let mut guard = fs::FS.lock();
    let (disk, auth, perms) = guard.disk_and_overlays_mut().expect("disk not mounted");
    fs::system_domain::load_at_boot(disk, auth, perms);
}

fn init_devices() {
    io::device::DEVICE_TABLE.lock().register("console");
}

fn seed_test_user() {
    let entry = fs::overlay::AuthEntry::new(1, b"test");
    fs::FS.lock().auth.insert(entry);
}

fn install_init_binary() {
    static INIT_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.bin"));
    let pid = scheduler::current_pid();
    let mut fs = fs::FS.lock();
    let fd = fs.create("/init", pid).expect("failed to create /init");
    fs.write(fd, pid, 0, 0, INIT_BIN).expect("failed to write /init");
    fs.close(fd, pid).expect("failed to close /init");
}
 
fn io_smoke_test() {
    let mut disk = RamDisk::new(16, 512);
    let mut buf = [0u8; 512];
    for (i, b) in buf.iter_mut().enumerate() { *b = (i % 256) as u8; }
    disk.write_sector(3, &buf).expect("write failed");
    let mut rbuf = [0u8; 512];
    disk.read_sector(3, &mut rbuf).expect("read failed");
    assert_eq!(buf, rbuf);
    serial_println!("io smoke test passed");
}
 
fn sync_syscall_smoke_test() {
    let mut tf = TrapFrame { r15:0, r14:0, r13:0, r12:0, rbp:0, rbx:0,
                              r11:0, r10:0, r9:0,  r8:0,  rdi:0, rsi:0,
                              rdx:0, rcx:0, rax:0, user_rsp:0 };
    let h = syscall_handler(SYS_GETRESOURCE, &mut tf) as usize;
    tf.rdi = h as u64; assert_eq!(syscall_handler(SYS_MUTEX_LOCK,    &mut tf),   0);
    tf.rdi = h as u64; assert_eq!(syscall_handler(SYS_MUTEX_TRYLOCK, &mut tf), -11);
    tf.rdi = h as u64; assert_eq!(syscall_handler(SYS_MUTEX_UNLOCK,  &mut tf),   0);
    tf.rdi = h as u64; assert_eq!(syscall_handler(SYS_MUTEX_TRYLOCK, &mut tf),   0);
    serial_println!("sync syscall smoke test passed");
}
 
fn fs_smoke_test() {
    let pid = scheduler::current_pid();
    let mut fs = fs::FS.lock();
    let fd = fs.create("/hello.txt", pid).expect("create failed");
    fs.write(fd, pid, 0, 0, b"hi").expect("write failed");
    fs.close(fd, pid).expect("close failed");
    let fd = fs.open("/hello.txt", pid, 0, 0).expect("reopen failed");
    let mut buf = [0u8; 2];
    fs.read(fd, pid, 0, 0, &mut buf).expect("read failed");
    assert_eq!(&buf, b"hi");
    fs.close(fd, pid).expect("close failed");
    serial_println!("fs smoke test passed");
}
 
fn security_smoke_test() {
    use crate::security::{AccessControl, Rights, ACCESS_CONTROL};

    let mut ac = ACCESS_CONTROL.lock();

    ac.set_rights(1, 42, Rights(Rights::READ | Rights::WRITE | Rights::OWNER));
    let r = ac.access(1, 42);
    assert!(r.contains(Rights::READ));
    assert!(r.contains(Rights::WRITE));
    assert!(r.contains(Rights::OWNER));

    assert!(ac.grant(1, 2, 42, Rights(Rights::READ)).is_ok());
    assert!(ac.access(2, 42).contains(Rights::READ));

    assert!(ac.revoke(1, 2, 42, Rights(Rights::READ)).is_ok());
    assert!(!ac.access(2, 42).contains(Rights::READ));

    ac.define_role(1, Rights(Rights::READ | Rights::EXECUTE));
    ac.assign_role(3, 1);
    let r3 = ac.access(3, 99);
    assert!(r3.contains(Rights::READ));
    assert!(r3.contains(Rights::EXECUTE));

    serial_println!("security smoke test passed");
}

fn disk_scheduling_smoke_test() {
    use crate::io::crc::crc32;
    use crate::io::scheduler::{CscanScheduler, FcfsScheduler, DiskRequest, RequestScheduler};

    // CRC-32: verify a known round trip.
    let data = b"AlaphiOS";
    let c = crc32(data);
    assert_eq!(crc32(data), c);
    assert_ne!(c, 0);

    // C-SCAN: set head to 6 via a dummy drain, then submit [2,8,3,10,1].
    let mut cscan = CscanScheduler::new();
    cscan.submit(DiskRequest { lba: 5, count: 1, is_write: false });
    let _ = cscan.drain();
    for &lba in &[2, 8, 3, 10, 1] {
        cscan.submit(DiskRequest { lba, count: 1, is_write: false });
    }
    let order: alloc::vec::Vec<u64> = cscan.drain().iter().map(|r| r.lba).collect();
    assert_eq!(order, alloc::vec![8, 10, 1, 2, 3]);

    let mut fcfs = FcfsScheduler::new();
    fcfs.submit(DiskRequest { lba: 5, count: 1, is_write: true });
    fcfs.submit(DiskRequest { lba: 6, count: 1, is_write: true });
    fcfs.submit(DiskRequest { lba: 7, count: 1, is_write: true });
    fcfs.submit(DiskRequest { lba: 3, count: 1, is_write: false });
    fcfs.submit(DiskRequest { lba: 10, count: 1, is_write: true });
    let merged = fcfs.drain();
    assert_eq!(merged.len(), 3);
    assert_eq!((merged[0].lba, merged[0].count, merged[0].is_write), (5, 3, true));
    assert_eq!((merged[1].lba, merged[1].count, merged[1].is_write), (3, 1, false));
    assert_eq!((merged[2].lba, merged[2].count, merged[2].is_write), (10, 1, true));

    serial_println!("disk scheduling smoke test passed");
}

fn consistency_smoke_test() {
    let guard = fs::FS.lock();
    let disk = guard.disk().expect("disk not mounted");
    fs::system_domain::test_consistency_repair(disk);
    drop(guard);
    serial_println!("consistency smoke test passed");
}

fn device_smoke_test() {
    use crate::io::device::{DEVICE_TABLE, CharDevice};
    use crate::io::console::CONSOLE;

    assert_eq!(DEVICE_TABLE.lock().find("console"), Some(0));
    assert!(DEVICE_TABLE.lock().is_blocking(0));

    CONSOLE.lock().put(b'!');

    serial_println!("device smoke test passed");
}

fn swap_smoke_test() {
    use crate::memory::vmm::{Vmm, VmArea, VmAreaKind};
    use crate::memory::frame_allocator::FrameAlloc;
    use x86_64::structures::paging::{Page, PageTableFlags};
    use x86_64::VirtAddr;

    let offset = VirtAddr::new(PHYS_MEM_OFFSET);
    let parent = unsafe { PageMapper::new(offset) };
    let (l4, mut mapper) = { let mut fa = FRAME_ALLOCATOR.lock(); parent.clone_kernel_half(&mut *fa) };
    let cr3 = l4.start_address().as_u64();

    let va = VirtAddr::new(0x5555_0000);
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    let mut vmm = Vmm::new();
    vmm.add_area(VmArea { start: va, end: va + 4096u64, flags, kind: VmAreaKind::Anonymous });

    {
        let mut fa = FRAME_ALLOCATOR.lock();
        let frame = fa.allocate().expect("swap test: OOM");
        let p = (PHYS_MEM_OFFSET + frame.start_address().as_u64()) as *mut u8;
        unsafe { for i in 0..4096 { p.add(i).write((i & 0xFF) as u8); } }
        mapper.map_page(Page::containing_address(va), frame, flags, &mut *fa);
    }

    assert!(memory::swap::evict_addr(&mut vmm, cr3, va.as_u64()), "evict failed");
    assert!(mapper.translate_addr(va).is_none(), "page still mapped after evict");

    {
        let mut fa = FRAME_ALLOCATOR.lock();
        vmm.handle_fault(va, &mut mapper, &mut *fa, false).expect("swap fault-in failed");
    }
    let phys = mapper.translate_addr(va).expect("page not mapped after fault-in");
    let p = (PHYS_MEM_OFFSET + phys.as_u64()) as *const u8;
    unsafe { for i in 0..4096 { assert_eq!(p.add(i).read(), (i & 0xFF) as u8, "swap data mismatch"); } }

    serial_println!("swap smoke test passed");
}

fn clone_smoke_test() {
    CLONE_FLAG.store(0);
    let curr = scheduler::current_tid();
    let thread = TASK_TABLE.lock().get(curr).unwrap().lock().clone_thread(clone_child_task, 0);
    let tid = thread.tid;
    scheduler::spawn(thread);
    lifecycle::wait_tid(tid);
    assert_eq!(CLONE_FLAG.load(), 1, "clone child did not run");
    serial_println!("clone/join smoke test passed");
}
 
fn fork_smoke_test() {
    FORK_FLAG.store(0);
    let curr = scheduler::current_tid();
    let phys_offset = x86_64::VirtAddr::new(PHYS_MEM_OFFSET);
    let parent_mapper = unsafe { PageMapper::new(phys_offset) };
    let (child_l4, _) = { let mut fa = FRAME_ALLOCATOR.lock(); parent_mapper.clone_kernel_half(&mut *fa) };
    let child_pid = {
        let table = TASK_TABLE.lock();
        let mut child = table.get(curr).unwrap().lock().fork_kernel(fork_child_task, 0);
        child.cr3 = child_l4.start_address().as_u64();
        let pid = child.pid;
        drop(table);
        scheduler::spawn(child);
        pid
    };
    let code = lifecycle::wait(child_pid);
    assert_eq!(code, 0);
    assert_eq!(FORK_FLAG.load(), 42, "fork child did not run");
    serial_println!("fork smoke test passed");
}
 
fn user_mode_smoke_test() {
    let mut task = process::loader::load_path("/init", 0);
    task.parent = Some(scheduler::current_pid());
    let pid = task.pid;
    scheduler::spawn(task);

    let code = lifecycle::wait(pid);
    assert_eq!(code as u64, pid.as_u64(), "user task returned wrong exit code");
    serial_println!("user-mode smoke test passed: ring-3 syscall round trip, pid={}", pid.as_u64());
}

fn clone_child_task() -> ! {
    CLONE_FLAG.store(1);
    lifecycle::exit(0);
}
 
fn fork_child_task() -> ! {
    FORK_FLAG.store(42);
    lifecycle::exit(0);
}
 
fn ping_pong_task(my_turn: u8) -> ! {
    loop {
        let mut guard = PING_PONG.lock();
        while guard.turn != my_turn { guard = PING_PONG.wait(guard); }
        guard.count += 1;
        let done = guard.count >= PING_PONG_HANDOFFS;
        guard.turn = 1 - my_turn;
        if done { serial_println!("scheduler/monitor smoke test passed: {} handoffs", guard.count); }
        PING_PONG.broadcast();
        core::mem::drop(guard);
        if done { lifecycle::exit(0); }
    }
}
 
fn ping_pong_a() -> ! { ping_pong_task(0) }
fn ping_pong_b() -> ! { ping_pong_task(1) }
 

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("KERNEL PANIC: {}", info);
    loop { x86_64::instructions::hlt(); }
}
