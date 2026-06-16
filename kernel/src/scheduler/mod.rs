pub mod mlfq;

use alloc::sync::Arc;
use spin::Lazy;
use crate::process::pid::{Pid, Tid};
use crate::process::table::TaskRegistry;
use crate::process::task::Task;
use crate::sync::spinlock::Spinlock;
use mlfq::MlfqScheduler;



static SCHEDULER: Lazy<Spinlock<MlfqScheduler>> = Lazy::new(|| Spinlock::new(MlfqScheduler::new()));


pub fn init() {
    let idle = Arc::new(Spinlock::new(Task::new(idle_task, (mlfq::NUM_QUEUES - 1) as u8, None)));
    SCHEDULER.lock().set_current(idle);
}


pub fn spawn(task: Task) {
    let arc = Arc::new(Spinlock::new(task));
    crate::process::table::TASK_TABLE.lock().insert(arc.clone());
    SCHEDULER.lock().push(arc);
}


pub fn tick() {
    SCHEDULER.lock().tick();
}

// Called from timer handler (interrupts already disabled by hardware).
// The temporary SpinlockGuard is dropped before switch_to is called, so the scheduler lock is free when the stack switch happens.
pub fn schedule() {
    if crate::process::task::preempt_count() > 0 {
        return;
    }

    let switch_pair = SCHEDULER.lock().prepare_switch();
    if let Some((out, inc)) = switch_pair {
        unsafe { crate::arch::context::switch_to(out, inc) };
    }
}


fn idle_task() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}


pub fn current_pid() -> Pid {
    SCHEDULER.lock().current_pid()
}


pub fn current_ppid() -> Option<Pid> {
    SCHEDULER.lock().current_ppid()
}


pub fn current_tid() -> Tid {
    SCHEDULER.lock().current_tid()
}


pub fn remove_current() {
    let _ = SCHEDULER.lock().remove_current();
}


pub fn unblock(pid: Pid) {
    if let Some(task_lock) = crate::process::table::TASK_TABLE.lock().get(pid) {
        {
            let mut task = task_lock.lock();
            if let crate::process::task::TaskState::Blocked = task.state {
                task.state = crate::process::task::TaskState::Ready;
            } else {
                return;
            }
        }
        SCHEDULER.lock().push(task_lock);
    }
}
