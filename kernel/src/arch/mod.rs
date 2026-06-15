// Everything here is x86-64-specific. 
// Nothing outside arch/ should call CPU instructions directly.
pub mod context;
pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod paging;


use bootloader_api::BootInfo;


pub fn init() {
    gdt::init();
    idt::init();
    interrupts::init();
}
