use alloc::collections::VecDeque;
use crate::process::pid::Pid;
use crate::process::task::TaskState;
use crate::process::table::{TASK_TABLE, TaskRegistry};
use crate::scheduler;
use crate::sync::spinlock::{Spinlock, SpinlockGuard};


pub struct CondVar {
    waiters: Spinlock<VecDeque<Pid>>,
}

impl CondVar {
    pub const fn new() -> Self { 
        Self {
            waiters: Spinlock::new(VecDeque::new()),
        }
    }

    pub fn wait<T>(&self, guard: SpinlockGuard<'_, T>) -> SpinlockGuard<'_, T> {
        let curr_pid = scheduler::current_pid();
        self.waiters.lock().push_back(curr_pid);
        let lock_ptr = guard.lock as *const Spinlock<T>;

        {
            let table = TASK_TABLE.lock();
            if let Some(task_lock) = table.get(curr_pid) {
                let mut task = task_lock.lock();
                task.state = TaskState::Blocked;
            }
        }

        core::mem::drop(guard);
        scheduler::schedule();

        // SAFETY: lock_ptr points to a valid Spinlock<T> instance managed by the caller.
        unsafe { (*lock_ptr).lock() }

    }

    pub fn signal(&self) {
        if let Some(pid) = self.waiters.lock().pop_front() {
            scheduler::unblock(pid);
        }
    }

    pub fn broadcast(&self) {
        let mut waiters_guard = self.waiters.lock();
        while let Some(pid) = waiters_guard.pop_front() {
            scheduler::unblock(pid);
        }
    }
}
