use alloc::collections::VecDeque;
use crate::process::pid::Tid;
use crate::process::task::TaskState;
use crate::process::table::{TASK_TABLE, TaskRegistry};
use crate::scheduler;
use crate::sync::spinlock::{Spinlock, SpinlockGuard};


pub struct CondVar {
    waiters: Spinlock<VecDeque<Tid>>,
}

impl CondVar {
    pub const fn new() -> Self {
        Self { waiters: Spinlock::new(VecDeque::new()) }
    }

    pub fn wait<T>(&self, guard: SpinlockGuard<'_, T>) -> SpinlockGuard<'_, T> {
        let curr_tid = scheduler::current_tid();
        self.waiters.lock().push_back(curr_tid);
        let lock_ptr = guard.lock as *const Spinlock<T>;

        {
            let table = TASK_TABLE.lock();
            if let Some(task_lock) = table.get(curr_tid) {
                task_lock.lock().state = TaskState::Blocked;
            }
        }

        core::mem::drop(guard);
        scheduler::schedule();

        unsafe { (*lock_ptr).lock() }
    }

    pub fn signal(&self) {
        if let Some(tid) = self.waiters.lock().pop_front() {
            scheduler::unblock_tid(tid);
        }
    }

    pub fn broadcast(&self) {
        let mut waiters = self.waiters.lock();
        while let Some(tid) = waiters.pop_front() {
            scheduler::unblock_tid(tid);
        }
    }
}
