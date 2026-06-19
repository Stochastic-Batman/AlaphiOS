use fatfs::{DefaultTimeProvider, FileSystem, FsOptions, IoBase, LossyOemCpConverter, Read, Seek, SeekFrom, Write};
use crate::io::disk::DiskDevice;


pub type FatDir<'a, D> = fatfs::Dir<'a, DiskIo<D>, DefaultTimeProvider, LossyOemCpConverter>;


// Cursor wrapper so fatfs can drive any DiskDevice.
pub struct DiskIo<D: DiskDevice> {
    dev: D,
    pos: u64,
    sector_buf: alloc::vec::Vec<u8>,
}

impl<D: DiskDevice> DiskIo<D> {
    pub fn new(dev: D) -> Self {
        let n = dev.sector_size();
        Self {
            dev,
            pos: 0,
            sector_buf: alloc::vec![0u8; n],
        }
    }
}


#[derive(Debug)]
pub struct FatIoError;


impl fatfs::IoError for FatIoError {
    fn is_interrupted(&self) -> bool { false }
    fn new_unexpected_eof_error() -> Self { FatIoError }
    fn new_write_zero_error() -> Self { FatIoError }
}


impl<D: DiskDevice> IoBase for DiskIo<D> {
    type Error = FatIoError;
}


impl<D: DiskDevice> Read for DiskIo<D> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let sector_size = self.dev.sector_size() as u64;
        let mut bytes_read = 0usize;

        while bytes_read < buf.len() {
            let lba = self.pos / sector_size;
            let offset_in_sector = (self.pos % sector_size) as usize;

            self.dev.read_sector(lba, &mut self.sector_buf).map_err(|_| FatIoError)?;

            let available = sector_size as usize - offset_in_sector;
            let to_copy = core::cmp::min(available, buf.len() - bytes_read);
            
            buf[bytes_read..bytes_read + to_copy].copy_from_slice(&self.sector_buf[offset_in_sector..offset_in_sector + to_copy]);
            bytes_read += to_copy;
            self.pos += to_copy as u64;
        }

        Ok(bytes_read)
    }
}


impl<D: DiskDevice> Write for DiskIo<D> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let sector_size = self.dev.sector_size() as u64;
        let mut bytes_written = 0usize;

        while bytes_written < buf.len() {
            let lba = self.pos / sector_size;
            let offset_in_sector = (self.pos % sector_size) as usize;

            self.dev.read_sector(lba, &mut self.sector_buf).map_err(|_| FatIoError)?;

            let available = sector_size as usize - offset_in_sector;
            let to_copy = core::cmp::min(available, buf.len() - bytes_written);
            
            self.sector_buf[offset_in_sector..offset_in_sector + to_copy].copy_from_slice(&buf[bytes_written..bytes_written + to_copy]);
            self.dev.write_sector(lba, &self.sector_buf).map_err(|_| FatIoError)?;
            bytes_written += to_copy;
            self.pos += to_copy as u64;
        }

        Ok(bytes_written)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // RamDisk writes are synchronous; nothing to flush.
        Ok(())
    }
}


impl<D: DiskDevice> Seek for DiskIo<D> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        let total = self.dev.sector_count() * self.dev.sector_size() as u64;
        self.pos = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::End(n) => (total as i64 + n) as u64,
            SeekFrom::Current(n) => (self.pos as i64 + n) as u64,
        };
        Ok(self.pos)
    }
}


pub struct FatFs<D: DiskDevice> {
    inner: FileSystem<DiskIo<D>, DefaultTimeProvider, LossyOemCpConverter>,
}

impl<D: DiskDevice> FatFs<D> {
    pub fn mount(dev: D) -> Result<Self, FatIoError> {
        let io = DiskIo::new(dev);
        let fs = FileSystem::new(io, FsOptions::new()).map_err(|_| FatIoError)?;
        Ok(Self { inner: fs })
    }

    pub fn root_dir(&self) -> FatDir<'_, D> { 
        self.inner.root_dir()
    }
}
