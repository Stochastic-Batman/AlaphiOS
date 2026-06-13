use x86_64::{PhysAddr, VirtAddr};
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, Translate,
};
use x86_64::registers::control::Cr3;


pub const PHYS_MEM_OFFSET: u64 = 0xFFFF_8000_0000_0000;  // Must match bootloader-config/bootloader.toml `physical-memory-offset`.


pub struct PageMapper {
    inner: OffsetPageTable<'static>,
}

impl PageMapper {
    pub unsafe fn new(phys_mem_offset: VirtAddr) -> Self {
        let (l4_table_frame, _) = Cr3::read();
        let l4_table_phys_addr = l4_table_frame.start_address();

        let l4_table_virt_addr = phys_mem_offset + l4_table_phys_addr.as_u64();

        let l4_table_ptr: *mut PageTable = l4_table_virt_addr.as_mut_ptr();
        let l4_table: &'static mut PageTable = &mut *l4_table_ptr;

        let inner = OffsetPageTable::new(l4_table, phys_mem_offset);

        Self { inner }
    }

    pub fn map_page(&mut self, page: Page<Size4KiB>, frame: PhysFrame, flags: PageTableFlags, allocator: &mut impl FrameAllocator<Size4KiB>) {
        unsafe {
            self.inner.map_to(page, frame, flags, allocator).expect("map_page failed").flush();
        }
    }

    pub fn unmap_page(&mut self, page: Page<Size4KiB>) -> PhysFrame {
        let (frame, mapper_flush) = unsafe {
            self.inner.unmap(page).expect("unmap_page failed")
        };

        mapper_flush.flush();
        frame
    }

    pub fn translate_addr(&self, virt: VirtAddr) -> Option<PhysAddr> {
        self.inner.translate_addr(virt)
    }

    pub fn flush_tlb(&self, page: Page<Size4KiB>) {
        x86_64::instructions::tlb::flush(page.start_address());
    }

    pub fn flush_tlb_all(&self) {
        x86_64::instructions::tlb::flush_all();
    }
}


/// Initialise paging and return the kernel's PageMapper.
/// Called from memory::init(), which passes in the offset from BootInfo.
pub fn init(phys_mem_offset: VirtAddr) -> PageMapper {
    unsafe { PageMapper::new(phys_mem_offset) }
}
