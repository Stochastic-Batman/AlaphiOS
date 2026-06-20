// LockHandle is a user-visible mutex + condvar pair reachable by a handle from SYS_GETRESOURCE. 
// unlike Spinlock/Monitor, holding the lock does not disable preemption and can span multiple syscalls:
// SYS_MUTEX_LOCK returns to user space still "held", released later by an unrelated SYS_MUTEX_UNLOCK call.
use alloc::collections::VecDeque;
use crate::process::pid::Pid;
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
    waiters: VecDeque<Pid>,
}

pub struct QueuedResource {
    state: Spinlock<MutexState>,
    cond_waiters: Spinlock<VecDeque<Pid>>,
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
            let curr_pid = scheduler::current_pid();
            let acquired = {
                let mut s = self.state.lock();
                if s.locked {
                    s.waiters.push_back(curr_pid);
                    mark_blocked(curr_pid);  // still under s: preemption disabled, no lost wakeup
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

        if let Some(pid) = waiter {
            scheduler::unblock(pid);
        }
    }

    fn wait(&self) {
        let curr_pid = scheduler::current_pid();

        {
            let mut cw = self.cond_waiters.lock();
            cw.push_back(curr_pid);
            mark_blocked(curr_pid);
        }

        self.unlock();
        scheduler::schedule();
        self.lock();
    }

    fn signal(&self) {
        let waiter = self.cond_waiters.lock().pop_front();
        if let Some(pid) = waiter {
            scheduler::unblock(pid);
        }
    }

    fn broadcast(&self) {
        loop {
            let waiter = self.cond_waiters.lock().pop_front();
            match waiter {
                Some(pid) => scheduler::unblock(pid),
                None => break,
            }
        }
    }
}


fn mark_blocked(pid: Pid) {
    let table = TASK_TABLE.lock();
    if let Some(task_lock) = table.get(pid) {
        task_lock.lock().state = TaskState::Blocked;
    }
}
