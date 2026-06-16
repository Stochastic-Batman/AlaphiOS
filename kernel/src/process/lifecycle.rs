use crate::process::pid::Pid;
use crate::process::task::TaskState;
use crate::process::table::{TASK_TABLE, TaskRegistry};
use crate::scheduler;


pub fn exit(exit_code: i32) -> ! {
    let curr_pid = scheduler::current_pid();

    if let Some(task_lock) = TASK_TABLE.lock().get(curr_pid) {
        let mut task = task_lock.lock();
        task.state = TaskState::Zombie;
        task.exit_code = Some(exit_code);
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
        let child_zombie = {
            let table = TASK_TABLE.lock();
            if let Some(task_lock) = table.get(child_pid) {
                match task_lock.lock().state {
                    TaskState::Zombie => true,
                    _ => false,
                }
            } else {
                return -1;
            }
        };

        if child_zombie {
            if let Some(task_lock) = TASK_TABLE.lock().remove(child_pid) {
                let child_task = task_lock.lock();
                return child_task.exit_code.unwrap_or(-1);  // Fallback value if None
            }
            return -1;
        }

        if let Some(task_lock) = TASK_TABLE.lock().get(curr_pid) {
            task_lock.lock().state = TaskState::Blocked;
        }

        scheduler::schedule();
    }
}
