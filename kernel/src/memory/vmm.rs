use alloc::vec::Vec;
use x86_64::VirtAddr;
use x86_64::structures::paging::{FrameAllocator, PageTableFlags, Size4KiB};
use crate::arch::paging::PageMapper;
use crate::memory::frame_allocator::FrameAlloc;



pub enum VmAreaKind {
    Anonymous,  // zero-fill-on-demand
}


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

    pub fn handle_fault(&mut self, fault_addr: VirtAddr, mapper: &mut PageMapper, frame_alloc: &mut (impl FrameAlloc + FrameAllocator<Size4KiB>)) -> Result<(), ()> {
        let area = self.areas.iter().find(|x| x.start <= fault_addr && fault_addr < x.end);
        let area = area.ok_or(())?;
        let frame = frame_alloc.allocate().ok_or(())?;

        // Zero the frame before mapping so anonymous pages start clean.
        let phys_offset = crate::arch::paging::PHYS_MEM_OFFSET;
        let virt = (phys_offset + frame.start_address().as_u64()) as *mut u8;
        unsafe { core::ptr::write_bytes(virt, 0, 4096); }

        let page = x86_64::structures::paging::Page::containing_address(fault_addr);
        mapper.map_page(page, frame, area.flags, frame_alloc);

        Ok(())
    }
}
