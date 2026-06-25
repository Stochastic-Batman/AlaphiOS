use alloc::vec;
use alloc::vec::Vec;
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::VirtAddr;
use crate::arch::paging::{PageMapper, PHYS_MEM_OFFSET};
use crate::io::crc::crc32;
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
    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    IoError,
    CrcMismatch,
}


const RAMDISK_VIRT_REGION_BASE: usize = 0xFFFF_D000_0000_0000;
const PAGE_SIZE: usize = 4096;
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
    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    cursor: u64,
    crcs: Vec<u32>,
    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    frames: Vec<PhysFrame<Size4KiB>>,
}


impl RamDisk {
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

        // Zero the whole region up front so a freshly formatted volume doesn't see stale physical memory contents.
        unsafe {
            core::ptr::write_bytes(base.as_mut_ptr::<u8>(), 0, byte_len);
        }

        let zero_crc = crc32(&vec![0u8; sector_size]);
        let crcs = vec![zero_crc; sector_count];

        Self { base, byte_len, sector_size, cursor: 0, crcs, frames }
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
        
        unsafe {
            core::ptr::copy_nonoverlapping(self.sector_ptr(lba), buf.as_mut_ptr(), self.sector_size);
        }
        
        if crc32(buf) != self.crcs[lba as usize] {
            return Err(DiskError::CrcMismatch);
        }
        
        Ok(())
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), DiskError> {
        let start = (lba as usize) * self.sector_size;
        
        if start + self.sector_size > self.byte_len {
            return Err(DiskError::OutOfBounds);
        }
        
        self.crcs[lba as usize] = crc32(buf);
        
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
