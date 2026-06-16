use crate::process::pid::Pid;
use crate::process::task::TaskState;
use crate::process::table::{TASK_TABLE, TaskRegistry};
use crate::scheduler;


pub fn exit(exit_code: i32) -> ! {
    let curr_pid = scheduler::current_pid();

    let parent_pid = {
        if let Some(task_lock) = TASK_TABLE.lock().get(curr_pid) {
            let mut task = task_lock.lock();
            task.state = TaskState::Zombie;
            task.exit_code = Some(exit_code);
            task.parent
        } else {
            None
        }
    };

    if let Some(pid) = parent_pid {
        scheduler::unblock(pid);
    }

    scheduler::remove_current();
    scheduler::schedule();

    loop {
        x86_64::instructions::hlt();
    }
}


pub fn wait(child_pid: Pid) -> i32 {
    let curr_pid = scheduler::current_pid();

    loop {
        let result = {
            let mut table = TASK_TABLE.lock();
            match table.get(child_pid) {
                None => Some(-1),
                Some(task_lock) => {
                    match task_lock.lock().state {
                        TaskState::Zombie => {
                            let exit_code = task_lock.lock().exit_code.unwrap_or(-1);  // Fallback value if None
                            table.remove(child_pid);
                            Some(exit_code)
                        }
                        _ => None,
                    }
                }
            }
        };

        if let Some(code) = result {
            return code;
        }

        // Child not yet zombie: block self and yield.
        if let Some(task_lock) = TASK_TABLE.lock().get(curr_pid) {
            task_lock.lock().state = TaskState::Blocked;
        }

        scheduler::schedule();
    }
}
