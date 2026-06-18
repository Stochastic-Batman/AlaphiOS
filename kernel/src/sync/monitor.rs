use crate::sync::spinlock::{Spinlock, SpinlockGuard};
use crate::sync::condvar::CondVar;


pub struct Monitor<T> {
    lock: Spinlock<T>,
    cond: CondVar,
}


impl<T> Monitor<T> {
    pub const fn new(val: T) -> Self { 
        Self {
            lock: Spinlock::new(val), 
            cond: CondVar::new(),
        }                                 
    }
    
    pub fn lock(&self) -> SpinlockGuard<'_, T> { 
        self.lock.lock() 
    }
    
    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> { 
        self.lock.try_lock() 
    }

    pub fn wait<'a>(&'a self, guard: SpinlockGuard<'a, T>) -> SpinlockGuard<'a, T> {
        self.cond.wait(guard)
    }

    pub fn signal(&self) {
        self.cond.signal();
    }

    pub fn broadcast(&self) {
        self.cond.broadcast();
    }
}
