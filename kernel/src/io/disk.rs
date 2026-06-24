// Decision 60: the boot-time backing store for the FAT32 system partition.
// This used to be a single Vec<u8>, which meant asking the kernel heap's
// buddy allocator (decision 52) for one multi-megabyte contiguous block --
// no buddy allocator should reasonably be sized to satisfy that in one
// block (decision 32's fixed-block-size design assumes requests stay well
// under the largest order). A disk image is page-granular physical memory,
// not heap data, so it's mapped the same way kernel heap pages are
// (see memory::heap::init): claim a disjoint virtual range (one per
// RamDisk instance, see claim_virt_range below), back each 4 KiB page with
// a frame from FrameAlloc, write through the mapping directly.
use alloc::vec::Vec;
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::VirtAddr;
use crate::arch::paging::{PageMapper, PHYS_MEM_OFFSET};
use crate::memory::frame_allocator::FrameAlloc;
use crate::sync::atomic::KernelAtomicUsize;


pub trait DiskDevice: Send {
    fn sector_size(&self) -> usize;
    fn sector_count(&self) -> u64;
    fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), DiskError>;
    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), DiskError>; 
}


#[derive(Debug)]
pub enum DiskError {
    OutOfBounds,
    IoError,
}


// Disjoint from HEAP_START (0xFFFF_C000_0000_0000) and the physical-memory
// offset mapping (0xFFFF_8000_0000_0000). Picked from the same upper-half
// canonical address family for consistency with the rest of the kernel's
// fixed virtual layout. This is the start of a region; individual RamDisk
// instances each claim a disjoint slice of it via RAMDISK_VIRT_CURSOR below.
const RAMDISK_VIRT_REGION_BASE: usize = 0xFFFF_D000_0000_0000;
const PAGE_SIZE: usize = 4096;

// Bumping cursor: each RamDisk::new() call claims the next free slice of
// virtual address space and advances this past it. Mirrors Pid::next() /
// Tid::next() -- monotonic, never reused, no bookkeeping needed since
// nothing currently tears down a RamDisk's mapping.
static RAMDISK_VIRT_CURSOR: KernelAtomicUsize = KernelAtomicUsize::new(RAMDISK_VIRT_REGION_BASE);

fn claim_virt_range(byte_len: usize) -> VirtAddr {
    let page_aligned_len = byte_len.div_ceil(PAGE_SIZE) * PAGE_SIZE;
    let base = RAMDISK_VIRT_CURSOR.fetch_add(page_aligned_len);
    VirtAddr::new(base as u64)
}


pub struct RamDisk {
    base: VirtAddr,
    byte_len: usize,
    sector_size: usize,
    cursor: u64,
    frames: Vec<PhysFrame<Size4KiB>>,  // retained so the mapping can be torn down later if needed
}


impl RamDisk {
    // Each call claims a fresh, disjoint virtual range (see
    // claim_virt_range), so multiple RamDisks -- e.g. a throwaway one in a
    // smoke test plus the real boot disk -- never collide. Requires the
    // global frame allocator / page mapper to be initialised, i.e. this
    // must run after memory::init().
    pub fn new(sector_count: usize, sector_size: usize) -> Self {
        let byte_len = sector_count * sector_size;
        let base = claim_virt_range(byte_len);

        let mut mapper = unsafe { PageMapper::new(VirtAddr::new(PHYS_MEM_OFFSET)) };
        let mut frame_alloc = crate::memory::FRAME_ALLOCATOR.lock();

        let start_page: Page<Size4KiB> = Page::containing_address(base);
        let end_page: Page<Size4KiB> = Page::containing_address(base + (byte_len as u64 - 1));
        let page_range = Page::range_inclusive(start_page, end_page);

        let mut frames = Vec::with_capacity(byte_len / PAGE_SIZE);
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        for page in page_range {
            let frame = frame_alloc.allocate().expect("out of physical frames for RamDisk");
            mapper.map_page(page, frame, flags, &mut *frame_alloc);
            frames.push(frame);
        }

        // Zero the whole region up front so a freshly formatted volume
        // doesn't see stale physical memory contents.
        unsafe {
            core::ptr::write_bytes(base.as_mut_ptr::<u8>(), 0, byte_len);
        }

        Self { base, byte_len, sector_size, cursor: 0, frames }
    }

    fn sector_ptr(&self, lba: u64) -> *mut u8 {
        (self.base.as_u64() + lba * self.sector_size as u64) as *mut u8
    }
}

impl DiskDevice for RamDisk {
    fn sector_size(&self) -> usize {
        self.sector_size
    }

    fn sector_count(&self) -> u64 {
        (self.byte_len / self.sector_size) as u64
    }

    fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), DiskError> {
        let start = (lba as usize) * self.sector_size;
        if start + self.sector_size > self.byte_len {
            return Err(DiskError::OutOfBounds);
        }

        // SAFETY: [start, start + sector_size) lies within the mapped
        // RamDisk region (checked above), which is PRESENT | WRITABLE for
        // its entire byte_len, set up once in `new`.
        unsafe {
            core::ptr::copy_nonoverlapping(self.sector_ptr(lba), buf.as_mut_ptr(), self.sector_size);
        }
        Ok(())
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), DiskError> {
        let start = (lba as usize) * self.sector_size;
        if start + self.sector_size > self.byte_len {
            return Err(DiskError::OutOfBounds);
        }

        // SAFETY: see read_sector.
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), self.sector_ptr(lba), self.sector_size);
        }
        Ok(())
    }
}


impl crate::io::device::BlockDevice for RamDisk {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, crate::io::device::DeviceError> {
        let offset = self.cursor as usize;
        let avail = self.byte_len.saturating_sub(offset);
        let n = buf.len().min(avail);
        if n == 0 { return Ok(0); }
        unsafe {
            core::ptr::copy_nonoverlapping(
                (self.base.as_u64() as usize + offset) as *const u8,
                buf.as_mut_ptr(),
                n,
            );
        }
        self.cursor += n as u64;
        Ok(n)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, crate::io::device::DeviceError> {
        let offset = self.cursor as usize;
        let avail = self.byte_len.saturating_sub(offset);
        let n = buf.len().min(avail);
        if n == 0 { return Ok(0); }
        unsafe {
            core::ptr::copy_nonoverlapping(
                buf.as_ptr(),
                (self.base.as_u64() as usize + offset) as *mut u8,
                n,
            );
        }
        self.cursor += n as u64;
        Ok(n)
    }

    fn seek(&mut self, offset: u64) {
        self.cursor = offset;
    }
}
