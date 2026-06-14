#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
// After heap is implemented: uncomment so Box/Vec/Arc work.
// extern crate alloc;

use bootloader_api::{entry_point, BootInfo};
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


// Entry point 
entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial_init();
    serial_println!("AlaphiOS booting...");

    arch::init(boot_info);
    serial_println!("arch init done");

    // memory::init(boot_info);
    // serial_println!("memory init done");

    // scheduler::init(); // (uncomment when scheduler/ is implemented):

    serial_println!("boot complete, entering idle loop");
    loop {
        x86_64::instructions::hlt();
    }
}


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
