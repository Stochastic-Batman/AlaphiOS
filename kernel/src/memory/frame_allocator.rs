use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::{PhysAddr};



pub trait FrameAlloc {
    fn allocate(&mut self) -> Option<PhysFrame<Size4KiB>>;

    fn deallocate(&mut self, frame: PhysFrame<Size4KiB>);
}


// Boot-time only. Used to map the heap before the free-list allocator exists.
// deallocate() is a no-op; frames handed out here are permanent kernel mappings.
pub struct BootFrameAllocator {
    regions: &'static MemoryRegions,
    next: usize,
}


impl BootFrameAllocator {
    // SAFETY: `regions` must be the memory map from BootInfo; 
    // caller guarantees the bootloader has not handed out any of the usable frames yet.
    pub unsafe fn new(regions: &'static MemoryRegions) -> Self {
        Self {
            regions,
            next: 0,
        }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame<Size4KiB>> + '_ {
        self.regions.iter().filter(|r| r.kind == MemoryRegionKind::Usable)
            .flat_map(|r| {
                let start_frame = PhysFrame::containing_address(PhysAddr::new(r.start));
                let end_frame = PhysFrame::containing_address(PhysAddr::new(r.end - 1));  // inclusive
                PhysFrame::range_inclusive(start_frame, end_frame)
            })
    }

    pub fn frames_consumed(&self) -> usize {
        self.next
    }
}


impl FrameAlloc for BootFrameAllocator {
    fn allocate(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let res = self.usable_frames().nth(self.next);
        self.next += 1;
        res
    }

    fn deallocate(&mut self, _frame: PhysFrame<Size4KiB>) {
        // No-op for boot allocator. VMM replaces this impl entirely.
    }
}

// The x86_64 crate's Mapper trait requires FrameAllocator<Size4KiB>.
unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for BootFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate()
    }
}
