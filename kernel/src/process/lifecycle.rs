use crate::process::pid::{Pid, Tid};
use crate::process::task::TaskState;
use crate::process::table::{TASK_TABLE, TaskRegistry};
use crate::scheduler;


pub fn exit(exit_code: i32) -> ! {
    let curr_tid = scheduler::current_tid();

    let parent_pid = {
        if let Some(task_lock) = TASK_TABLE.lock().get(curr_tid) {
            let mut task = task_lock.lock();
            task.state = TaskState::Zombie;
            task.exit_code = Some(exit_code);
            task.parent
        } else {
            None
        }
    };

    if let Some(pid) = parent_pid {
        scheduler::unblock_pid(pid);
    }

    scheduler::remove_current();
    scheduler::schedule();

    loop { x86_64::instructions::hlt(); }
}


// Used for process wait (fork). Scans by pid since forked children have unique pids.
pub fn wait(child_pid: Pid) -> i32 {
    let curr_tid = scheduler::current_tid();

    loop {
        let result = {
            let mut table = TASK_TABLE.lock();
            let found_tid = table.all_tids().into_iter().find(|&t| {
                table.get(t).map(|task| task.lock().pid == child_pid).unwrap_or(false)
            });
            match found_tid {
                None => Some(-1),
                Some(tid) => {
                    let task_lock = table.get(tid).unwrap();
                    let task = task_lock.lock();
                    match task.state {
                        TaskState::Zombie => {
                            let code = task.exit_code.unwrap_or(-1);
                            drop(task);
                            table.remove(tid);
                            Some(code)
                        }
                        _ => None,
                    }
                }
            }
        };

        if let Some(code) = result { return code; }

        if let Some(task_lock) = TASK_TABLE.lock().get(curr_tid) {
            task_lock.lock().state = TaskState::Blocked;
        }
        scheduler::schedule();
    }
}


// Used for thread join. Direct tid lookup — no scan needed.
pub fn wait_tid(target_tid: Tid) -> i32 {
    let curr_tid = scheduler::current_tid();

    loop {
        let result = {
            let mut table = TASK_TABLE.lock();
            match table.get(target_tid) {
                None => Some(-1),
                Some(task_lock) => {
                    let task = task_lock.lock();
                    match task.state {
                        TaskState::Zombie => {
                            let code = task.exit_code.unwrap_or(-1);
                            drop(task);
                            table.remove(target_tid);
                            Some(code)
                        }
                        _ => None,
                    }
                }
            }
        };

        if let Some(code) = result { return code; }

        if let Some(task_lock) = TASK_TABLE.lock().get(curr_tid) {
            task_lock.lock().state = TaskState::Blocked;
        }
        scheduler::schedule();
    }
}
