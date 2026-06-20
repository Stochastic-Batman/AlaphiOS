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
    region_idx: usize,
    cursor: Option<PhysFrame<Size4KiB>>,  // next candidate frame within regions[region_idx]
    frames_consumed: usize,
}


impl BootFrameAllocator {
    // SAFETY: `regions` must be the memory map from BootInfo; 
    // caller guarantees the bootloader has not handed out any of the usable frames yet.
    pub unsafe fn new(regions: &'static MemoryRegions) -> Self {
        let mut alloc = Self {
            regions,
            region_idx: 0,
            cursor: None,
            frames_consumed: 0,
        };
        alloc.seek_to_first_usable_frame();
        alloc
    }

    fn seek_to_first_usable_frame(&mut self) {
        while self.region_idx < self.regions.len() {
            let region = &self.regions[self.region_idx];
            if region.kind == MemoryRegionKind::Usable {
                let start_frame = PhysFrame::containing_address(PhysAddr::new(region.start));
                let end_frame = PhysFrame::containing_address(PhysAddr::new(region.end - 1));  // inclusive
 
                let candidate = self.cursor.unwrap_or(start_frame);
                if candidate <= end_frame {
                    self.cursor = Some(candidate);
                    return;
                }
            }
 
            self.region_idx += 1;
            self.cursor = None;
        }
 
        self.cursor = None;  // exhausted: no usable frames left
    }

    pub fn frames_consumed(&self) -> usize {
        self.frames_consumed
    }
}


impl FrameAlloc for BootFrameAllocator {
    fn allocate(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = self.cursor?;
        
        self.cursor = Some(frame + 1);
        self.frames_consumed += 1;
        self.seek_to_first_usable_frame();

        Some(frame)
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
