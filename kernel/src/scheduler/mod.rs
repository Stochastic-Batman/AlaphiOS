pub mod mlfq;

use alloc::sync::Arc;
use spin::Lazy;
use crate::arch::syscall::PerCpuScratch;
use crate::process::pid::{Pid, Tid};
use crate::process::table::{TaskRegistry, TASK_TABLE};
use crate::process::task::{Task, TaskState};
use crate::sync::spinlock::Spinlock;
use mlfq::MlfqScheduler;
use x86_64::registers::model_specific::KernelGsBase;


static SCHEDULER: Lazy<Spinlock<MlfqScheduler>> = Lazy::new(|| Spinlock::new(MlfqScheduler::new()));


pub fn init() {
    let boot_task = Arc::new(Spinlock::new(Task::new(idle_task, 0, None)));

    {
        let guard = boot_task.lock();
        let addr = &guard.scratch as *const PerCpuScratch as u64;
        unsafe { KernelGsBase::write(x86_64::VirtAddr::new(addr)); }
    }

    TASK_TABLE.lock().insert(boot_task.clone());
    SCHEDULER.lock().set_current(boot_task);

    spawn(Task::new(idle_task, (mlfq::NUM_QUEUES - 1) as u8, None));
}


pub fn spawn(task: Task) {
    let arc = Arc::new(Spinlock::new(task));
    TASK_TABLE.lock().insert(arc.clone());
    SCHEDULER.lock().push(arc);
}


pub fn tick() {
    SCHEDULER.lock().tick();
}


pub fn schedule() {
    if crate::process::task::preempt_count() > 0 {
        return;
    }
    SCHEDULER.lock().should_reschedule();
    let switch_pair = SCHEDULER.lock().prepare_switch();
    if let Some((out, inc)) = switch_pair {
        unsafe { crate::arch::context::switch_to(out, inc) };
    }
}


fn idle_task() -> ! {
    loop { x86_64::instructions::hlt(); }
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


// Direct tid lookup - used by condvar, resource, wait_tid.
pub fn unblock_tid(tid: Tid) {
    if let Some(task_lock) = TASK_TABLE.lock().get(tid) {
        {
            let mut task = task_lock.lock();
            if let TaskState::Blocked = task.state {
                task.state = TaskState::Ready;
            } else {
                return;
            }
        }
        SCHEDULER.lock().push(task_lock);
    }
}


// Pid scan - used by lifecycle::exit to wake a waiting parent.
pub fn unblock_pid(pid: Pid) {
    let tid = {
        let table = TASK_TABLE.lock();
        let tids = table.all_tids();
        tids.into_iter().find(|&t| {
            table.get(t).map(|task| task.lock().pid == pid).unwrap_or(false)
        })
    };
    if let Some(tid) = tid {
        unblock_tid(tid);
    }
}
