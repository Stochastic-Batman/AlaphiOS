use alloc::vec::Vec;
use spin::Lazy;
use crate::sync::spinlock::Spinlock;


#[derive(Debug)]
#[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
pub enum DeviceError {
    NotFound,
    IoError,
}

#[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
pub trait BlockDevice: Send {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, DeviceError>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, DeviceError>;
    fn seek(&mut self, offset: u64);
}

pub trait CharDevice: Send {
    fn get(&mut self) -> Option<u8>;
    fn put(&mut self, byte: u8);
}


pub const DEV_FD_OFFSET: u64 = 0x1_0000;
pub const IOCTL_SET_NONBLOCK: usize = 1;
pub const IOCTL_SET_BLOCK: usize = 2;


pub struct DeviceStatus {
    pub name: &'static str,
    pub blocking: bool,
}

pub struct DeviceTable {
    entries: Vec<DeviceStatus>,
}

impl DeviceTable {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn register(&mut self, name: &'static str) -> usize {
        let idx = self.entries.len();
        self.entries.push(DeviceStatus { name, blocking: true });
        idx
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name == name)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut DeviceStatus> {
        self.entries.get_mut(idx)
    }

    pub fn is_blocking(&self, idx: usize) -> bool {
        self.entries.get(idx).map(|e| e.blocking).unwrap_or(true)
    }
}

pub static DEVICE_TABLE: Lazy<Spinlock<DeviceTable>> = Lazy::new(|| Spinlock::new(DeviceTable::new()));
