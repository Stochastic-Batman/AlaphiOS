// kernel heap backed by the buddy allocator, mapped by the page mapper into a fixed virtual address range.
use x86_64::structures::paging::{FrameAllocator, Page, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;
use crate::arch::paging::PageMapper;
use crate::memory::frame_allocator::FrameAlloc;
use crate::memory::buddy_allocator::LockedBuddyAllocator;


pub const HEAP_START: usize = 0xFFFF_C000_0000_0000;
pub const HEAP_SIZE: usize = 8 * 1024 * 1024;

#[global_allocator]
static ALLOCATOR: LockedBuddyAllocator = LockedBuddyAllocator::new();


pub fn init(mapper: &mut PageMapper, frame_alloc: &mut (impl FrameAlloc + FrameAllocator<Size4KiB>)) {
    let start_page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(HEAP_START as u64));
    let end_page: Page<Size4KiB> = Page::containing_address(VirtAddr::new((HEAP_START + HEAP_SIZE - 1) as u64));    
    let page_range = Page::range_inclusive(start_page, end_page);

    for page in page_range {
        let frame = frame_alloc.allocate().expect("Failed to allocate physical frame for kernel heap");
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        mapper.map_page(page, frame, flags, frame_alloc);
    }

    unsafe {
        ALLOCATOR.init(HEAP_START, HEAP_SIZE);
    }
}
