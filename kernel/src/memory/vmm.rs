use alloc::vec::Vec;
use x86_64::VirtAddr;
use x86_64::structures::paging::{FrameAllocator, PageTableFlags, Size4KiB, Page, PhysFrame};
use crate::arch::paging::PageMapper;
use crate::memory::frame_allocator::FrameAlloc;



#[derive(Clone, Copy)]
pub enum VmAreaKind {
    Anonymous,  // zero-fill-on-demand
}

#[derive(Clone, Copy)]
pub struct VmArea {
    pub start: VirtAddr,
    pub end:   VirtAddr,   // exclusive
    pub flags: PageTableFlags,
    pub kind:  VmAreaKind,
}


pub struct Vmm {
    areas: Vec<VmArea>,
}


impl Vmm {
    pub fn new() -> Self {
        Self { areas: Vec::new() }
    }

    pub fn add_area(&mut self, area: VmArea) {
        self.areas.push(area);
    }

    pub fn handle_fault(&mut self, fault_addr: VirtAddr, mapper: &mut PageMapper, frame_alloc: &mut (impl FrameAlloc + FrameAllocator<Size4KiB>), is_write: bool) -> Result<(), ()> {
        let area = self.areas.iter().find(|a| a.start <= fault_addr && fault_addr < a.end).ok_or(())?;
        let offset = crate::arch::paging::PHYS_MEM_OFFSET;
        let page = x86_64::structures::paging::Page::containing_address(fault_addr);

        if let Some(phys) = mapper.translate_addr(fault_addr) {
            // Page is present but faulted: COW - must be a writable area mapped read-only.
            if !is_write || !area.flags.contains(PageTableFlags::WRITABLE) {
                return Err(());
            }

            let new_frame = frame_alloc.allocate().ok_or(())?;
            
            unsafe {
                core::ptr::copy_nonoverlapping(
                    (offset + phys.as_u64()) as *const u8,
                    (offset + new_frame.start_address().as_u64()) as *mut u8,
                    4096,
                );
            }
            
            mapper.unmap_page(page);
            mapper.map_page(page, new_frame, area.flags, frame_alloc);
        } else {
            // Page absent: demand-paging, zero-fill.
            let new_frame = frame_alloc.allocate().ok_or(())?;
            unsafe { 
                core::ptr::write_bytes((offset + new_frame.start_address().as_u64()) as *mut u8, 0, 4096);
            }
            mapper.map_page(page, new_frame, area.flags, frame_alloc);
        }

        Ok(())
    }

    pub fn fork_eager(&self, child_mapper: &mut PageMapper, frame_alloc: &mut (impl FrameAlloc + FrameAllocator<Size4KiB>), parent_mapper: &PageMapper) -> Result<Vmm, ()> {
        let mut child_vmm = Vmm::new();
        let phys_offset = crate::arch::paging::PHYS_MEM_OFFSET;

        for area in &self.areas {
            child_vmm.add_area(VmArea {
                start: area.start,
                end: area.end,
                flags: area.flags,
                kind: VmAreaKind::Anonymous,
            });

            let mut addr = area.start;
            while addr < area.end {
                let page = x86_64::structures::paging::Page::containing_address(addr);
                // only copy pages that are actually mapped in the parent
                if let Some(phys) = parent_mapper.translate_addr(addr) {
                    let new_frame = frame_alloc.allocate().ok_or(())?;
                    let src = (phys_offset + phys.as_u64()) as *const u8;
                    let dst = (phys_offset + new_frame.start_address().as_u64()) as *mut u8;
                   
                    unsafe { core::ptr::copy_nonoverlapping(src, dst, 4096); }
                    child_mapper.map_page(page, new_frame, area.flags, frame_alloc);
                }

                addr += 4096u64;
            }
        }

        Ok(child_vmm)
    }

    pub fn fork_cow(&mut self, child_mapper: &mut PageMapper, parent_mapper: &mut PageMapper, frame_alloc: &mut (impl FrameAlloc + FrameAllocator<Size4KiB>)) -> Vmm {
        let mut child = Vmm::new();

        for area in &self.areas {
            child.add_area(*area);  // child inherits same flags
            let ro = area.flags & !PageTableFlags::WRITABLE;
            let mut addr = area.start;
            
            while addr < area.end {
                let page = Page::containing_address(addr);
                
                if let Some(phys) = parent_mapper.translate_addr(addr) {
                    let frame = PhysFrame::containing_address(phys);
                    parent_mapper.remap_flags(page, ro);  // parent now read-only
                    child_mapper.map_page(page, frame, ro, frame_alloc);  // child shares same frame
                }
                
                addr += 4096u64;
            }
        }

        child
    }
}
