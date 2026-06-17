use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use crate::memory::frame_allocator::FrameAlloc;


pub struct FreeListAllocator {
    head: Option<PhysAddr>,
    free_count: usize,
    phys_offset: u64,
}


impl FreeListAllocator {
    pub const fn empty() -> Self {
        Self { 
            head: None, 
            free_count: 0, 
            phys_offset: 0 
        }
    }

    // SAFETY: all usable frames in `regions` must be unused and reachable via `phys_offset`. 
    pub unsafe fn init(&mut self, regions: &MemoryRegions, phys_offset: u64, mut skip: usize) {
        self.phys_offset = phys_offset;
        for region in regions.iter() {
            if region.kind != MemoryRegionKind::Usable {
                continue;
            }
            
            let start = PhysAddr::new(region.start);
            let end = PhysAddr::new(region.end);
            let mut addr = start;
            
            while addr + 4096u64 <= end {
                if skip > 0 {
                    skip -= 1;
                } else {
                    self.deallocate(PhysFrame::containing_address(addr));
                }

                addr += 4096u64;
            }
        }
    }

    pub fn free_count(&self) -> usize {
        self.free_count
    }

    fn frame_to_virt(&self, frame: PhysFrame) -> *mut u64 {
        (self.phys_offset + frame.start_address().as_u64()) as *mut u64
    }
}


impl FrameAlloc for FreeListAllocator {
    fn allocate(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let head_phys = self.head?;
        let head_virt = (self.phys_offset + head_phys.as_u64()) as *const u64;
        let next_phys = unsafe { core::ptr::read(head_virt) };

        self.head = match next_phys { 
            0 => None, 
            _ => Some(PhysAddr::new(next_phys)) 
        };
        self.free_count -= 1;
        
        Some(PhysFrame::containing_address(head_phys))
    }

    fn deallocate(&mut self, frame: PhysFrame<Size4KiB>) {
        let virt = self.frame_to_virt(frame);
        let old_head = self.head.map(|x| x.as_u64()).unwrap_or(0);

        unsafe { core::ptr::write(virt, old_head); }

        self.head = Some(frame.start_address());
        self.free_count += 1;
    }
}


unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for FreeListAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate()
    }
}
