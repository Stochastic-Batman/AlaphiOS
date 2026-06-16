use alloc::collections::VecDeque;
use alloc::sync::Arc;
use crate::process::pid::Pid;
use crate::process::task::{Task, TaskState};
use crate::sync::spinlock::Spinlock;


pub const NUM_QUEUES: usize = 8;
const BOOST_PERIOD: u32 = 100;  // every BOOST_PERIOD ticks all tasks are moved to the upper queue to prevent starvation.
const TIME_SLICES: [u32; NUM_QUEUES] = {
    let mut slices = [0; NUM_QUEUES];
    let mut i = 0;
    while i < NUM_QUEUES {
        let x = (i + 1) as u32;
        slices[i] = x * x;
        i += 1;
    }
    slices
};


pub struct MlfqScheduler {
    queues: [VecDeque<Arc<Spinlock<Task>>>; NUM_QUEUES],
    current: Option<Arc<Spinlock<Task>>>,
    ticks_remaining: u32,
    total_ticks: u32,
    needs_reschedule: bool,
}

impl MlfqScheduler {
    pub fn new() -> Self {
        Self {
            queues: core::array::from_fn(|_| VecDeque::new()),
            current: None,
            ticks_remaining: TIME_SLICES[0],
            total_ticks: 0,
            needs_reschedule: false,
        }
    }

    pub fn current_pid(&self) -> Pid {
        self.current.as_ref().map(|task| task.lock().pid).expect("No current task running")
    }

    // Mark the task Running, store it in self.current and set ticks_remaining = TIME_SLICES[task.priority].
    pub fn set_current(&mut self, task: Arc<Spinlock<Task>>) {
        let priority = {
            let mut guard = task.lock();
            guard.state = TaskState::Running;
            guard.priority as usize
        };

        self.current = Some(task);
        self.ticks_remaining = TIME_SLICES[priority];
    }

    pub fn remove_current(&mut self) -> Option<Arc<Spinlock<Task>>> {
        if self.current.is_some() {
            self.ticks_remaining = 0;
            self.needs_reschedule = true;
        }
        self.current.take()
    }

    pub fn push(&mut self, task: Arc<Spinlock<Task>>) {
        let priority = {
            let guard = task.lock();
            guard.priority as usize
        };

        if priority < NUM_QUEUES {
            self.queues[priority].push_back(task);
        } else {
            self.queues[NUM_QUEUES - 1].push_back(task);
        }
    }

    pub fn tick(&mut self) {
        self.total_ticks += 1;

        if self.total_ticks % BOOST_PERIOD == 0 {
            self.boost_priorities();
        }

        if self.current.is_some() {
            if self.ticks_remaining > 0 {
                self.ticks_remaining -= 1;
            }

            if self.ticks_remaining == 0 {
                self.needs_reschedule = true;
            }
        }
    }

    pub fn needs_reschedule(&self) -> bool {
        self.needs_reschedule
    }

    pub fn prepare_switch(&mut self) -> Option<(*mut Task, *const Task)> {
        if !self.needs_reschedule {
           return None;
        }

        let arc_in = match self.pick_next() {
            Some(task) => task,
            None => {
                self.needs_reschedule = false;
                return None;
            }
        };

        let out_ptr: *mut Task = if let Some(arc_out) = self.current.take() {
            let prio = {
                let mut guard = arc_out.lock();
                let p = core::cmp::min((guard.priority + 1) as usize, NUM_QUEUES - 1);
                guard.priority = p as u8;
                guard.state = TaskState::Ready;
                p
            };

            self.queues[prio].push_back(arc_out.clone());
            arc_out.as_ptr()
        } else {
            core::ptr::null_mut()
        };

        let prio_in = {
            let mut guard = arc_in.lock();
            guard.state = TaskState::Running;
            guard.priority as usize
        };

        let in_ptr: *const Task = arc_in.as_ptr();

        self.current = Some(arc_in);
        self.ticks_remaining = TIME_SLICES[prio_in];
        self.needs_reschedule = false;

        Some((out_ptr, in_ptr))
    }

    fn pick_next(&mut self) -> Option<Arc<Spinlock<Task>>> {
        for q in self.queues.iter_mut() {
            if !q.is_empty() {
                return q.pop_front();
            }
        }

        None
    }

    fn boost_priorities(&mut self) {
        let mut promoted_queues: [VecDeque<Arc<Spinlock<Task>>>; NUM_QUEUES] = core::array::from_fn(|_| VecDeque::new());

        for i in 1..NUM_QUEUES {
            let mut q = core::mem::take(&mut self.queues[i]);  // take the whole queue to avoid borrowing issues while iterating and modifying self.queues[0]

            while let Some(task_arc) = q.pop_front() {        
                {
                    let mut task_guard = task_arc.lock();
                    task_guard.priority = (i - 1) as u8;
                }

                promoted_queues[i - 1].push_back(task_arc);
            }
        }

        for i in 0..(NUM_QUEUES - 1) {
            self.queues[i].append(&mut promoted_queues[i]);
        }
    }
}
