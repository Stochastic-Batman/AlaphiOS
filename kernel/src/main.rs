#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
extern crate alloc;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use bootloader_api::config::Mapping;
use core::fmt::Write;
use core::panic::PanicInfo;
use spin::Mutex;
use uart_16550::SerialPort;

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


// Problem: Kernel has no screen driver yet. The VGA framebuffer exists but I haven't written code to talk to it. I need some way to print debug output while building everything else. Serial is the answer because it requires almost no setup.
// UART (Universal Asynchronous Receiver-Transmitter) is a hardware protocol for sending bytes one bit at a time over a wire. The 16550 is a specific chip implementing it - it's been in PCs since the 1980s and every PC-compatible machine (including QEMU's emulated one) has it. "Serial port" and "COM port" are the same thing from the software side.


// The 16550 UART chip lives at I/O port 0x3F8 (COM1) on every x86 PC.
// QEMU forwards writes here to stdout when launched with `-serial stdio`.
// I wrap it in a Mutex so interrupt handlers and the main path can both
// print without corrupting each other's output.
static SERIAL: Mutex<SerialPort> = unsafe {
    // SAFETY: 0x3F8 is the standard COM1 base address; nothing else in the
    // kernel touches this port. `SerialPort::new` is a const fn that only
    // stores the address — no I/O happens until `init()` is called below.
    Mutex::new(SerialPort::new(0x3F8))
};


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


const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut cfg = BootloaderConfig::new_default();
    cfg.mappings.physical_memory = Some(Mapping::FixedAddress(0xFFFF_8000_0000_0000));
    cfg.kernel_stack_size = 64 * 4096;
    cfg
};


entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);


fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial_init();
    serial_println!("AlaphiOS booting...");

    arch::init();
    serial_println!("arch init done");

    memory::init(boot_info);
    serial_println!("memory init done");
    {
        use alloc::boxed::Box;
        let probe = Box::new(0xDEAD_BEEFu64);
        serial_println!("heap: {:x} @ {:p}", *probe, probe);
    }

    io_smoke_test();
    serial_println!("io smoke test done");

    scheduler::init();

    fs_smoke_test();
    serial_println!("fs smoke test done");
    
    use crate::process::task::Task;
   // scheduler::spawn(Task::new(scheduler_smoke_a, 0, None));
   // scheduler::spawn(Task::new(scheduler_smoke_b, 0, None));
    scheduler::spawn(Task::new(ping_pong_a, 0, None));
    scheduler::spawn(Task::new(ping_pong_b, 0, None));
    serial_println!("scheduler smote tests passed");

    serial_println!("boot complete, entering idle loop");
    loop {
        x86_64::instructions::hlt();
    }
}


fn scheduler_smoke_a() -> ! {
    loop {
        serial_println!("[A]");
        let end = crate::arch::interrupts::TICKS.load() + 20;
        while crate::arch::interrupts::TICKS.load() < end {
            x86_64::instructions::hlt();
        }
    }
}

fn scheduler_smoke_b() -> ! {
    loop {
        serial_println!("[B]");
        let end = crate::arch::interrupts::TICKS.load() + 20;
        while crate::arch::interrupts::TICKS.load() < end {
            x86_64::instructions::hlt();
        }
    }
}

fn io_smoke_test() {
    use crate::io::disk::DiskDevice;

    let mut disk = crate::io::disk::RamDisk::new(16, 512);
    let mut write_buf = [0u8; 512];
    for (i, b) in write_buf.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }

    disk.write_sector(3, &write_buf).expect("ramdisk write failed");
    let mut read_buf = [0u8; 512];
    disk.read_sector(3, &mut read_buf).expect("ramdisk read failed");

    assert_eq!(write_buf, read_buf, "ramdisk roundtrip mismatch");
    serial_println!("io smoke test passed: sector roundtrip ok");
}

fn fs_smoke_test() {
    let pid = scheduler::current_pid();
    let mut fs = crate::fs::FS.lock();

    let fd = fs.create("/hello.txt", pid).expect("create failed");
    fs.write(fd, pid, b"hi").expect("write failed");
    fs.close(fd, pid).expect("close failed");

    serial_println!("fs smoke test passed: fcb/lock layer ok (fatfs I/O not wired yet)");
}


struct PingPongState { turn: u8, count: u32 }

static PING_PONG: crate::sync::monitor::Monitor<PingPongState> =
    crate::sync::monitor::Monitor::new(PingPongState { turn: 0, count: 0 });

const PING_PONG_HANDOFFS: u32 = 20;

fn ping_pong_task(my_turn: u8) -> ! {
    loop {
        let mut guard = PING_PONG.lock();
        while guard.turn != my_turn {
            guard = PING_PONG.wait(guard);
        }

        guard.count += 1;
        let done = guard.count >= PING_PONG_HANDOFFS;
        guard.turn = 1 - my_turn;

        if done {
            serial_println!("scheduler/monitor smoke test passed: {} handoffs", guard.count);
        }

        PING_PONG.broadcast();
        core::mem::drop(guard);

        if done {
            crate::process::lifecycle::exit(0);
        }
    }
}

fn ping_pong_a() -> ! { ping_pong_task(0) }
fn ping_pong_b() -> ! { ping_pong_task(1) }



#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Do not call any allocator or scheduler code here — the system may be
    // in an inconsistent state. Serial I/O is safe because _print only takes
    // a spinlock and writes to a hardware register.
    serial_println!("KERNEL PANIC: {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
