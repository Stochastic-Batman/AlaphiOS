pub mod buddy_allocator;
pub mod frame_allocator;
pub mod heap;
// pub mod vmm;
// pub mod swap;


use bootloader_api::BootInfo;
use x86_64::VirtAddr;
use crate::arch::paging;
use frame_allocator::BootFrameAllocator;


/// Top-level memory subsystem init. Called from kernel_main after arch::init().
pub fn init(boot_info: &'static mut BootInfo) {
    let phys_mem_offset = VirtAddr::new(
        boot_info.physical_memory_offset.into_option().expect("bootloader did not provide physical memory offset")
    );

    let mut mapper = paging::init(phys_mem_offset);
    let mut frame_alloc = unsafe { BootFrameAllocator::new(&boot_info.memory_regions) };
    heap::init(&mut mapper, &mut frame_alloc);

    // After this point `alloc` crate types (Box, Vec, Arc) are usable.
}
