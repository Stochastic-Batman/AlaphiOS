use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use crate::sync::resource::LockHandle;
use crate::sync::spinlock::Spinlock;


pub struct ResourceTable {
    next_handle: u64,
    map: BTreeMap<u64, Arc<dyn LockHandle>>,
}

impl ResourceTable {
    pub const fn new() -> Self {
        Self {
            next_handle: 0u64,
            map: BTreeMap::<u64, Arc<dyn LockHandle>>::new(),
        }
    }

    pub fn insert(&mut self, res: Arc<dyn LockHandle>) -> u64 {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.checked_add(1).expect("resource handle overflow");
        self.map.insert(handle, res);
        handle
    }

    pub fn get(&self, handle: u64) -> Option<Arc<dyn LockHandle>> {
        self.map.get(&handle).cloned()
    }

    pub fn remove(&mut self, handle: u64) {
        self.map.remove(&handle);
    }
}

pub static RESOURCE_TABLE: Spinlock<ResourceTable> = Spinlock::new(ResourceTable::new());
