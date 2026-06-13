use crate::arch::gdt::DOUBLE_FAULT_IST_INDEX;
use crate::{serial_println};
use spin::Lazy;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode}; 


#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = 0x20,
    Keyboard = 0x21,
}


static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    unsafe {
        idt.double_fault.set_handler_fn(double_fault_handler).set_stack_index(DOUBLE_FAULT_IST_INDEX);
    }
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.general_protection_fault.set_handler_fn(gpf_handler);
    idt[InterruptIndex::Timer as usize].set_handler_fn(timer_handler);
    idt[InterruptIndex::Keyboard as usize].set_handler_fn(keyboard_handler);
    idt
});


pub fn init() {
    IDT.load();
}


// Exception handlers
extern "x86-interrupt" fn breakpoint_handler(frame: InterruptStackFrame) {
    serial_println!("BREAKPOINT\n{:#?}", frame);
}

extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, _error: u64) -> ! {
    panic!("DOUBLE FAULT\n{:#?}", frame);
}

extern "x86-interrupt" fn page_fault_handler(frame: InterruptStackFrame, error: PageFaultErrorCode) {
    panic!("PAGE FAULT\nAddress: {:?}\nError: {:?}\n{:#?}", Cr2::read(), error, frame);
}

extern "x86-interrupt" fn gpf_handler(frame: InterruptStackFrame, error: u64) {
    panic!("GENERAL PROTECTION FAULT\nError: {:#x}\n{:#?}", error, frame);
}


// IRQ handlers (thin FLIHs; heavy work deferred to SLIHs)
extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    crate::arch::interrupts::tick();
    crate::arch::interrupts::end_of_interrupt(InterruptIndex::Timer as u8);
}

extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let scancode: u8 = unsafe { Port::new(0x60).read() };
    crate::arch::interrupts::push_scancode(scancode);
    crate::arch::interrupts::end_of_interrupt(InterruptIndex::Keyboard as u8);
}
