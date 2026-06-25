// LockHandle is a user-visible mutex + condvar pair reachable by a handle from SYS_GETRESOURCE. 
// unlike Spinlock/Monitor, holding the lock does not disable preemption and can span multiple syscalls:
// SYS_MUTEX_LOCK returns to user space still "held", released later by an unrelated SYS_MUTEX_UNLOCK call.
use alloc::collections::VecDeque;
#[allow(unused_imports)]  // not used currently, but I believe this should be present for completeness.
use crate::process::pid::{Pid, Tid};
use crate::process::table::{TaskRegistry, TASK_TABLE};
use crate::process::task::TaskState;
use crate::scheduler;
use crate::sync::spinlock::Spinlock;


pub trait LockHandle: Send + Sync {
    fn lock(&self);
    fn try_lock(&self) -> bool;
    fn unlock(&self);
    fn wait(&self);  // caller must already hold the lock
    fn signal(&self);
    fn broadcast(&self);
}


// locked + waiters share one Spinlock so "check, then enqueue" is atomic.
// two separate locks here would let unlock() run in between and drop a wakeup.
struct MutexState {
    locked: bool,
    waiters: VecDeque<Tid>,
}

pub struct QueuedResource {
    state: Spinlock<MutexState>,
    cond_waiters: Spinlock<VecDeque<Tid>>,
}

impl QueuedResource {
    pub const fn new() -> Self {
        Self {
            state: Spinlock::new(MutexState { locked: false, waiters: VecDeque::new() }),
            cond_waiters: Spinlock::new(VecDeque::new()),
        }
    }
}

impl LockHandle for QueuedResource {
    fn lock(&self) {
        loop {
            let curr_tid = scheduler::current_tid();
            let acquired = {
                let mut s = self.state.lock();
                if s.locked {
                    s.waiters.push_back(curr_tid);
                    mark_blocked(curr_tid);  // still under s: preemption disabled, no lost wakeup
                    false
                } else {
                    s.locked = true;
                    true
                }
            };

            if acquired {
                return;
            }

            scheduler::schedule();
        }
    }

    fn try_lock(&self) -> bool {
        let mut s = self.state.lock();
        if s.locked {
            false
        } else {
            s.locked = true;
            true
        }
    }
    
    fn unlock(&self) {
        let waiter = {
            let mut s = self.state.lock();
            s.locked = false;
            s.waiters.pop_front()
        };

        if let Some(tid) = waiter {
            scheduler::unblock_tid(tid);
        }
    }

    fn wait(&self) {
        let curr_tid = scheduler::current_tid();

        {
            self.cond_waiters.lock().push_back(curr_tid);
            mark_blocked(curr_tid);
        }

        self.unlock();
        scheduler::schedule();
        self.lock();
    }

    fn signal(&self) {
        if let Some(tid) = self.cond_waiters.lock().pop_front() {
            scheduler::unblock_tid(tid);
        }
    }

    fn broadcast(&self) {
        loop {
            match self.cond_waiters.lock().pop_front() {
                Some(tid) => scheduler::unblock_tid(tid),
                None => break,
            }
        }
    }
}


fn mark_blocked(tid: Tid) {
    let table = TASK_TABLE.lock();
    if let Some(task_lock) = table.get(tid) {
        task_lock.lock().state = TaskState::Blocked;
    }
}
