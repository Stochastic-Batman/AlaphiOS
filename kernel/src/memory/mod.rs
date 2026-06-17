pub mod buddy_allocator;
pub mod frame_allocator;
pub mod free_list;
pub mod heap;
pub mod vmm;
pub mod swap;


use bootloader_api::BootInfo;
use spin::Lazy;
use x86_64::VirtAddr;
use crate::arch::paging;
use crate::sync::spinlock::Spinlock;
use frame_allocator::BootFrameAllocator;
use free_list::FreeListAllocator;


pub static FRAME_ALLOCATOR: Lazy<Spinlock<FreeListAllocator>> = Lazy::new(|| Spinlock::new(FreeListAllocator::empty()));


pub fn init(boot_info: &'static mut BootInfo) {
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().expect("bootloader did not provide physical memory offset"));
    let mut mapper = paging::init(phys_mem_offset);
    let mut boot_alloc = unsafe { BootFrameAllocator::new(&boot_info.memory_regions) };

    heap::init(&mut mapper, &mut boot_alloc);

    let skip = boot_alloc.frames_consumed();

    // Heap is live. Populate the free-list allocator from the memory map.
    // Frames consumed by the boot allocator for the heap are not tracked here;
    // those are permanent kernel mappings and never freed.
    unsafe {
        FRAME_ALLOCATOR.lock().init(&boot_info.memory_regions, phys_mem_offset.as_u64(), skip);
    }
}
