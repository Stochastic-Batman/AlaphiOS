use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use crate::sync::spinlock::Spinlock;
use super::channel::IpcChannel;


pub struct IpcTable {
    next_handle: u64,
    map: BTreeMap<u64, Arc<dyn IpcChannel>>,
}

impl IpcTable {
    pub const fn new() -> Self {
        Self {
            next_handle: 0u64,
            map: BTreeMap::<u64, Arc<dyn IpcChannel>>::new(),
        }
    }

    pub fn insert(&mut self, ch: Arc<dyn IpcChannel>) -> u64 {
        let old_handle = self.next_handle;
        self.next_handle = self.next_handle.checked_add(1).expect("IPC handle overflow");
        self.map.insert(old_handle, ch);
        old_handle
    }

    pub fn get(&self, handle: u64) -> Option<Arc<dyn IpcChannel>> {
        self.map.get(&handle).cloned()
    }

    pub fn remove(&mut self, handle: u64) { 
        self.map.remove(&handle);
    }
}

pub static IPC_TABLE: Spinlock<IpcTable> = Spinlock::new(IpcTable::new());
