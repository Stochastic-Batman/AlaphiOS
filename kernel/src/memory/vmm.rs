use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use x86_64::VirtAddr;
use x86_64::structures::paging::{FrameAllocator, PageTableFlags, Size4KiB, Page, PhysFrame};
#[allow(unused_imports)]  // not used currently, but I believe this should be present for completeness.
use crate::arch::paging::{PageMapper, PHYS_MEM_OFFSET};
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
    swapped: BTreeMap<u64, (u64, PageTableFlags)>,  // page vaddr -> (swap slot, restore flags)
}


impl Vmm {
    pub fn new() -> Self {
        Self { areas: Vec::new(), swapped: BTreeMap::new() }
    }

    pub fn add_area(&mut self, area: VmArea) {
        self.areas.push(area);
    }

    pub fn areas(&self) -> &[VmArea] {
        &self.areas
    }

    pub fn record_swapped(&mut self, page_vaddr: u64, slot: u64, flags: PageTableFlags) {
        self.swapped.insert(page_vaddr, (slot, flags));
    }

    pub fn handle_fault(&mut self, fault_addr: VirtAddr, mapper: &mut PageMapper, frame_alloc: &mut (impl FrameAlloc + FrameAllocator<Size4KiB>), is_write: bool) -> Result<(), ()> {
        let offset = crate::arch::paging::PHYS_MEM_OFFSET;
        let page = x86_64::structures::paging::Page::containing_address(fault_addr);

        let page_vaddr = page.start_address().as_u64();
        if let Some((slot, flags)) = self.swapped.remove(&page_vaddr) {
            let new_frame = frame_alloc.allocate().ok_or(())?;
            crate::memory::swap::swap_in(slot, new_frame);
            mapper.map_page(page, new_frame, flags, frame_alloc);
            return Ok(());
        }

        let area = self.areas.iter().find(|a| a.start <= fault_addr && fault_addr < a.end).ok_or(())?;

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

    pub fn set_heap_end(&mut self, heap_start: VirtAddr, new_end: VirtAddr, flags: PageTableFlags) {
        if let Some(area) = self.areas.iter_mut().find(|a| a.start == heap_start) {
            area.end = new_end;
        } else if new_end > heap_start {
            self.add_area(VmArea { start: heap_start, end: new_end, flags, kind: VmAreaKind::Anonymous });
        }
    }

    pub fn shrink_heap(&mut self, heap_start: VirtAddr, new_end: VirtAddr, old_end: VirtAddr, flags: PageTableFlags, mapper: &mut PageMapper, fa: &mut impl FrameAlloc) {
        let mut addr = new_end;
        
        while addr < old_end {
            if mapper.translate_addr(addr).is_some() {
                let page = Page::containing_address(addr);
                let frame = mapper.unmap_page(page);
                fa.deallocate(frame);
            }
            
            addr += 4096u64;
        }

        self.set_heap_end(heap_start, new_end, flags);
    }

    pub fn mprotect(&mut self, addr: VirtAddr, len: u64, new_flags: PageTableFlags, mapper: &mut PageMapper) -> Result<(), ()> {
        let end = addr + len;
        let idx = self.areas.iter().position(|a| a.start <= addr && addr < a.end).ok_or(())?;
        let area = self.areas[idx];

        if end > area.end {
            return Err(());
        }

        let mut replacements = Vec::new();
        if addr > area.start {
            replacements.push(VmArea { start: area.start, end: addr, flags: area.flags, kind: area.kind });
        }

        replacements.push(VmArea { start: addr, end, flags: new_flags, kind: area.kind });
        if end < area.end {
            replacements.push(VmArea { start: end, end: area.end, flags: area.flags, kind: area.kind });
        }

        self.areas.remove(idx);
        for (i, a) in replacements.into_iter().enumerate() {
            self.areas.insert(idx + i, a);
        }

        let mut page_addr = addr;
        
        while page_addr < end {
            if mapper.translate_addr(page_addr).is_some() {
                let page = Page::containing_address(page_addr);
                mapper.remap_flags(page, new_flags);
            }
        
            page_addr += 4096u64;
        }

        Ok(())
    }

    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
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
