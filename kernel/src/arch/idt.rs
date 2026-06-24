use crate::arch::gdt::DOUBLE_FAULT_IST_INDEX;
use crate::{serial_println};
use crate::process::table::TaskRegistry;
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
    idt[InterruptIndex::Timer as u8].set_handler_fn(timer_handler);
    idt[InterruptIndex::Keyboard as u8].set_handler_fn(keyboard_handler);
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
    let fault_addr = match Cr2::read() {
        Ok(addr) => addr,
        Err(_) => panic!("PAGE FAULT: could not read CR2\n{:#?}", frame),
    };

    let is_write = error.contains(PageFaultErrorCode::CAUSED_BY_WRITE);

    let handled = (|| -> Option<bool> {
        let task_arc = crate::process::table::TASK_TABLE.lock().get(crate::scheduler::current_tid())?;
        let mut task = task_arc.lock();
        let mut frame_alloc = crate::memory::FRAME_ALLOCATOR.lock();
        let phys_offset = x86_64::VirtAddr::new(crate::arch::paging::PHYS_MEM_OFFSET);
        let mut mapper = unsafe { crate::arch::paging::PageMapper::new(phys_offset) };

        Some(task.vmm.handle_fault(fault_addr, &mut mapper, &mut *frame_alloc, is_write).is_ok())
    })().unwrap_or(false);

    if !handled {
        panic!("PAGE FAULT\nAddress: {:?}\nError: {:?}\n{:#?}", fault_addr, error, frame);
    }
}

extern "x86-interrupt" fn gpf_handler(frame: InterruptStackFrame, error: u64) {
    panic!("GENERAL PROTECTION FAULT\nError: {:#x}\n{:#?}", error, frame);
}


// IRQ handlers (thin FLIHs; heavy work deferred to SLIHs)
extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    crate::arch::interrupts::tick();
    crate::arch::interrupts::end_of_interrupt(InterruptIndex::Timer as u8);  // EOI before schedule() so the PIC is not blocked if switch_to suspends this handler mid-flight.
    crate::scheduler::tick();
    crate::scheduler::schedule();
}

extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let scancode: u8 = unsafe { Port::new(0x60).read() };
    crate::arch::interrupts::push_scancode(scancode);
    crate::arch::interrupts::end_of_interrupt(InterruptIndex::Keyboard as u8);
}
