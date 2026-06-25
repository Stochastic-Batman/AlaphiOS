use alloc::sync::Arc;
use crate::sync::spinlock::Spinlock;
use crate::ipc::channel::{IpcChannel, IpcError};


pub struct SharedMem {
    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub name: alloc::string::String,
    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub data: Arc<Spinlock<alloc::vec::Vec<u8>>>,
}


impl SharedMem {
    pub fn new(name: alloc::string::String, size: usize) -> Self {
        let buf = alloc::vec![0u8; size];
        Self {
            name,
            data: Arc::new(Spinlock::new(buf)),
        }
    }
}


// SharedMem can be safely managed by the uniform IPC_TABLE
impl IpcChannel for SharedMem {
    fn send(&self, _data: &[u8]) -> Result<(), IpcError> {
        // Shared memory handles cannot be written to using stream-based syscalls
        Err(IpcError::WouldBlock)
    }

    fn recv(&self, _out: &mut [u8]) -> Result<usize, IpcError> {
        // Shared memory handles cannot be read from using stream-based syscalls
        Err(IpcError::WouldBlock)
    }

    fn close(&self) {
        // Handled cleanly when its reference count drops to 0 via IPC_TABLE removal
    }
}
