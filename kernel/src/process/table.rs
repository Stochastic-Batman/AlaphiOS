use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use crate::process::pid::Pid;
use crate::process::task::Task;
use crate::sync::spinlock::Spinlock;


pub trait TaskRegistry {
    fn insert(&mut self, task: Arc<Spinlock<Task>>);
    fn get(&self, pid: Pid) -> Option<Arc<Spinlock<Task>>>;
    fn remove(&mut self, pid: Pid) -> Option<Arc<Spinlock<Task>>>;
    fn all_pids(&self) -> alloc::vec::Vec<Pid>;
}

pub struct BTreeTaskRegistry {
    map: BTreeMap<Pid, Arc<Spinlock<Task>>>,
}


impl BTreeTaskRegistry {
    pub const fn new() -> Self { 
        Self { map: BTreeMap::new() } 
    }
}

impl TaskRegistry for BTreeTaskRegistry {
    fn insert(&mut self, task: Arc<Spinlock<Task>>) {
        let pid = task.lock().pid;
        self.map.insert(pid, task);
    }
    
    fn get(&self, pid: Pid) -> Option<Arc<Spinlock<Task>>> { 
        self.map.get(&pid).cloned() 
    }
    
    fn remove(&mut self, pid: Pid) -> Option<Arc<Spinlock<Task>>> { 
        self.map.remove(&pid) 
    }
    
    fn all_pids(&self) -> alloc::vec::Vec<Pid> { 
        self.map.keys().cloned().collect() 
    }
}


pub static TASK_TABLE: Spinlock<BTreeTaskRegistry> = Spinlock::new(BTreeTaskRegistry::new());
