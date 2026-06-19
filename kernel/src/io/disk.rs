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


pub struct RamDisk {
    data: alloc::vec::Vec<u8>,
    sector_size: usize,
}


impl RamDisk {
    pub fn new(sector_count: usize, sector_size: usize) -> Self {
        Self {
            data: alloc::vec![0u8; sector_count * sector_size],
            sector_size,
        }
    }
}

impl DiskDevice for RamDisk {
    fn sector_size(&self) -> usize {
        self.sector_size
    }

    fn sector_count(&self) -> u64 {
        (self.data.len() / self.sector_size) as u64
    }

    fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), DiskError> {
        let start = (lba as usize) * self.sector_size;
        let end = start + self.sector_size;

        if end > self.data.len() {
            return Err(DiskError::OutOfBounds);
        }

        buf[..self.sector_size].copy_from_slice(&self.data[start..end]);
        Ok(())
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), DiskError> {
        let start = (lba as usize) * self.sector_size;
        let end = start + self.sector_size;

        if end > self.data.len() {
            return Err(DiskError::OutOfBounds);
        }
        
        self.data[start..end].copy_from_slice(&buf[..self.sector_size]);
        Ok(())
    }
}
