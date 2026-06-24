use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use crate::process::pid::{Pid, Tid};
use crate::process::task::Task;
use crate::sync::spinlock::Spinlock;


pub trait TaskRegistry {
    fn insert(&mut self, task: Arc<Spinlock<Task>>);
    fn get(&self, tid: Tid) -> Option<Arc<Spinlock<Task>>>;
    fn remove(&mut self, tid: Tid) -> Option<Arc<Spinlock<Task>>>;
    fn all_tids(&self) -> alloc::vec::Vec<Tid>;
}

pub struct BTreeTaskRegistry {
    map: BTreeMap<Tid, Arc<Spinlock<Task>>>,
}

impl BTreeTaskRegistry {
    pub const fn new() -> Self {
        Self { map: BTreeMap::new() }
    }
}

impl TaskRegistry for BTreeTaskRegistry {
    fn insert(&mut self, task: Arc<Spinlock<Task>>) {
        let tid = task.lock().tid;
        self.map.insert(tid, task);
    }
    fn get(&self, tid: Tid) -> Option<Arc<Spinlock<Task>>> {
        self.map.get(&tid).cloned()
    }
    fn remove(&mut self, tid: Tid) -> Option<Arc<Spinlock<Task>>> {
        self.map.remove(&tid)
    }
    fn all_tids(&self) -> alloc::vec::Vec<Tid> {
        self.map.keys().cloned().collect()
    }
}


pub static TASK_TABLE: Spinlock<BTreeTaskRegistry> = Spinlock::new(BTreeTaskRegistry::new());
