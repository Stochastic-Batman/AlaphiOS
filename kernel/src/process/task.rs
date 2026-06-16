use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::process::pid::{Pid, Tid};
use x86_64::registers::control::Cr3;



const KERNEL_STACK_SIZE: usize = 64 * 1024;
static PREEMPT_COUNT: AtomicU32 = AtomicU32::new(0);  // stub: replaced by per-task field access via current_task() once the scheduler owns CURRENT.


#[unsafe(naked)]
unsafe extern "C" fn task_trampoline() {
    core::arch::naked_asm!(
        "sti",
        "ret",
    )
}


pub enum TaskState {
    Running,
    Ready,
    Blocked,
    Zombie,
}


pub struct Task {
    pub pid: Pid,
    pub tid: Tid,
    pub thread_group: Pid,
    pub state: TaskState,
    pub rsp: u64,
    pub cr3: u64,
    pub preempt_count: u32,  // TODO: replace global PREEMPT_COUNT stub with this field once current_task() exists in the scheduler
    pub priority: u8,
    pub parent: Option<Pid>,
    pub children: Vec<Pid>,
    pub exit_code: Option<i32>,
    kernel_stack: Vec<u8>,
}


impl Task {
    pub fn new(entry: fn() -> !, priority: u8, parent: Option<Pid>) -> Self {
        let mut kernel_stack = vec![0u8; KERNEL_STACK_SIZE];
        let rsp = Self::setup_stack(&mut kernel_stack, entry);
        let pid = Pid::next();

        Task {
            pid,
            tid: Tid::next(),
            thread_group: pid,
            state: TaskState::Ready,
            rsp,
            cr3: Cr3::read().0.start_address().as_u64(),  // page directory base register; stores phys addr of the root page table.
            preempt_count: 0,
            priority,
            parent,
            children: Vec::new(),
            exit_code: None,
            kernel_stack,
        }
    }

    fn setup_stack(stack: &mut Vec<u8>, entry: fn() -> !) -> u64 {
        let top = (stack.as_mut_ptr() as usize + stack.len()) & !15;  // left_op & 11110000 clears the lower 4 bits, aligning it as x86_64 demands.
        unsafe {  // these callee-saved registers must not change (or change, but must be restored before the function exits)
            let ptr = top as *mut u64;
            ptr.sub(1).write(entry as u64); // consumed by trampoline's ret
            ptr.sub(2).write(task_trampoline as u64);  // consumed by switch_to's ret
            ptr.sub(3).write(0); // rbx
            ptr.sub(4).write(0); // rbp
            ptr.sub(5).write(0); // r12
            ptr.sub(6).write(0); // r13
            ptr.sub(7).write(0); // r14
            ptr.sub(8).write(0); // r15
        }
        (top - 8 * 8) as u64
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
