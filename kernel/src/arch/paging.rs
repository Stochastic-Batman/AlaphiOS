use x86_64::{PhysAddr, VirtAddr};
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, Translate,
};
use x86_64::registers::control::Cr3;
use spin::Once;


pub const PHYS_MEM_OFFSET: u64 = 0xFFFF_8000_0000_0000;  // Must match bootloader-config/bootloader.toml `physical-memory-offset`.


static KERNEL_L4_PRESENT: Once<[bool; 512]> = Once::new();


// The bootloader maps the kernel image low (L4 entry 2), so cloning a fixed 256..512 range would drop the kernel's own code.
// This snapshots every present top-level entry once at boot, before any user address space exists, so clone_kernel_half can reproduce exactly the kernel mappings.
pub fn record_kernel_l4_entries() {
    let offset = VirtAddr::new(PHYS_MEM_OFFSET);
    let (cur_frame, _) = Cr3::read();
    let l4: &PageTable = unsafe { &*(offset + cur_frame.start_address().as_u64()).as_ptr() };
    let mut present = [false; 512];
    for i in 0..512 { present[i] = !l4[i].is_unused(); }
    KERNEL_L4_PRESENT.call_once(|| present);
}


pub struct PageMapper {
    inner: OffsetPageTable<'static>,
}

impl PageMapper {
    pub unsafe fn new(phys_mem_offset: VirtAddr) -> Self {
        let (l4_table_frame, _) = Cr3::read();
        let l4_table_phys_addr = l4_table_frame.start_address();

        let l4_table_virt_addr = phys_mem_offset + l4_table_phys_addr.as_u64();

        let l4_table_ptr: *mut PageTable = l4_table_virt_addr.as_mut_ptr();

        let inner = unsafe {
            let l4_table: &'static mut PageTable = &mut *l4_table_ptr;
            OffsetPageTable::new(l4_table, phys_mem_offset)
        };

        Self { inner }
    }

    pub fn map_page(&mut self, page: Page<Size4KiB>, frame: PhysFrame, flags: PageTableFlags, allocator: &mut impl FrameAllocator<Size4KiB>) {
        unsafe {
            self.inner.map_to(page, frame, flags, allocator).expect("map_page failed").flush();
        }
    }

    pub fn unmap_page(&mut self, page: Page<Size4KiB>) -> PhysFrame {
        let (frame, mapper_flush) = self.inner.unmap(page).expect("unmap_page failed");
        mapper_flush.flush();
        frame
    }

    pub fn translate_addr(&self, virt: VirtAddr) -> Option<PhysAddr> {
        self.inner.translate_addr(virt)
    }

    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub fn flush_tlb(&self, page: Page<Size4KiB>) {
        x86_64::instructions::tlb::flush(page.start_address());
    }

    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub fn flush_tlb_all(&self) {
        x86_64::instructions::tlb::flush_all();
    }

    pub unsafe fn new_from_frame(frame: PhysFrame, phys_mem_offset: VirtAddr) -> Self {
        let l4_virt = phys_mem_offset + frame.start_address().as_u64();
        let inner = unsafe { OffsetPageTable::new(&mut *l4_virt.as_mut_ptr(), phys_mem_offset) };
        Self { inner }
    }

    pub fn clone_kernel_half(&self, frame_alloc: &mut impl FrameAllocator<Size4KiB>) -> (PhysFrame, Self) {
        let offset = VirtAddr::new(PHYS_MEM_OFFSET);
        let new_frame = frame_alloc.allocate_frame().expect("OOM: L4 frame");

        // zero first, then copy kernel half
        unsafe { 
            core::ptr::write_bytes((offset + new_frame.start_address().as_u64()).as_mut_ptr::<u8>(), 0, 4096); 
        }

        let (cur_frame, _) = Cr3::read();
        let cur_l4: &PageTable = unsafe { &*(offset + cur_frame.start_address().as_u64()).as_ptr() };
        let new_l4: &mut PageTable = unsafe { &mut *(offset + new_frame.start_address().as_u64()).as_mut_ptr() };

        let kernel = KERNEL_L4_PRESENT.get().expect("kernel L4 entries not recorded");
        for i in 0..512 {
            if kernel[i] { new_l4[i] = cur_l4[i].clone(); }
        }

        let child_mapper = unsafe { Self::new_from_frame(new_frame, offset) };
        (new_frame, child_mapper)
    }

    pub fn remap_flags(&mut self, page: Page<Size4KiB>, new_flags: PageTableFlags) {
        unsafe {
            self.inner.update_flags(page, new_flags).expect("remap_flags: page not mapped").flush();
        }
    }

    // Reads the leaf PTE flags for a mapped address, used by the swap reaper to
    // inspect the accessed and dirty bits during enhanced second-chance selection.
    pub fn page_flags(&self, addr: VirtAddr) -> Option<PageTableFlags> {
        use x86_64::structures::paging::mapper::TranslateResult;
        match self.inner.translate(addr) {
            TranslateResult::Mapped { flags, .. } => Some(flags),
            _ => None,
        }
    }

    // Clears the accessed bit so a referenced page gets a second chance on the
    // next reaper sweep, and flushes the TLB so the CPU re-sets it on next touch.
    pub fn clear_accessed(&mut self, page: Page<Size4KiB>) {
        if let Some(flags) = self.page_flags(page.start_address()) {
            if flags.contains(PageTableFlags::ACCESSED) {
                unsafe {
                    if let Ok(f) = self.inner.update_flags(page, flags & !PageTableFlags::ACCESSED) {
                        f.flush();
                    }
                }
            }
        }
    }
}


/// Initialise paging and return the kernel's PageMapper.
/// Called from memory::init(), which passes in the offset from BootInfo.
pub fn init(phys_mem_offset: VirtAddr) -> PageMapper {
    unsafe { PageMapper::new(phys_mem_offset) }
}
