use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::process::pid::{Pid, Tid};
use x86_64::registers::control::Cr3;



const KERNEL_STACK_SIZE: usize = 64 * 1024;
static PREEMPT_COUNT: AtomicU32 = AtomicU32::new(0);  // stub: replaced by per-task field access via current_task() once the scheduler owns CURRENT.

pub enum TaskState {
    Running,
    Ready,
    Blocked,
    Zombie,
}


pub struct Task {
    pub pid: Pid,
    pub tid: Tid,
    pub state: TaskState,
    pub rsp: u64,
    pub cr3: u64,
    pub preempt_count: u32,
    pub priority: u8,
    pub parent: Option<Pid>,
    pub children: Vec<Pid>,
    kernel_stack: Vec<u8>,
}


impl Task {
    pub fn new(entry: fn() -> !, priority: u8, parent: Option<Pid>) -> Self {
        let mut kernel_stack = vec![0u8; KERNEL_STACK_SIZE];
        let rsp = Self::setup_stack(&mut kernel_stack, entry);
        Task {
            pid: Pid::next(),
            tid: Tid::next(),
            state: TaskState::Ready,
            rsp,
            cr3: Cr3::read().0.start_address().as_u64(),  // page directory base register; stores phys addr of the root page table.
            preempt_count: 0,
            priority,
            parent,
            children: Vec::new(),
            kernel_stack,
        }
    }

    fn setup_stack(stack: &mut Vec<u8>, entry: fn() -> !) -> u64 {
        let top = (stack.as_mut_ptr() as usize + stack.len()) & !15;  // left_op & 11110000 clears the lower 4 bits, aligning it as x86_64 demands.
        unsafe {  // these callee-saved registers must not change (or change, but must be restored before the function exits)
            let ptr = top as *mut u64;
            ptr.sub(1).write(entry as u64); // ret addr consumed by switch_to's `ret`
            ptr.sub(2).write(0); // rbx
            ptr.sub(3).write(0); // rbp
            ptr.sub(4).write(0); // r12
            ptr.sub(5).write(0); // r13
            ptr.sub(6).write(0); // r14
            ptr.sub(7).write(0); // r15
        }
        (top - 7 * 8) as u64
    }
}


pub fn preempt_disable() {
    PREEMPT_COUNT.fetch_add(1, Ordering::Relaxed);
}

pub fn preempt_enable() {
    PREEMPT_COUNT.fetch_sub(1, Ordering::Relaxed);
}

pub fn preempt_count() -> u32 {
    PREEMPT_COUNT.load(Ordering::Relaxed)
}
