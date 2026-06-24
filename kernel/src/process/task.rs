use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::arch::syscall::PerCpuScratch;
use crate::memory::vmm::Vmm;
use crate::process::pid::{Pid, Tid};
use crate::process::table::{TaskRegistry, TASK_TABLE};
use x86_64::registers::control::Cr3;



const KERNEL_STACK_SIZE: usize = 64 * 1024;
static PREEMPT_COUNT: AtomicU32 = AtomicU32::new(0);


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
    pub vmm: Vmm,
    pub kernel_stack_top: u64,
    pub scratch: PerCpuScratch,
    pub user_entry: u64,  // ring-3 RIP; 0 for pure kernel tasks
    pub user_stack: u64,  // ring-3 RSP; 0 for pure kernel tasks
    pub heap_start: u64,  // first page after code; 0 for kernel tasks
    pub brk: u64,         // current program break (grows from heap_start)
    kernel_stack: Vec<u8>,
}


impl Task {
    pub fn new(entry: fn() -> !, priority: u8, parent: Option<Pid>) -> Self {
        let mut kernel_stack = vec![0u8; KERNEL_STACK_SIZE];
        let kernel_stack_top = (kernel_stack.as_mut_ptr() as usize + kernel_stack.len()) as u64 & !15;
        let rsp = Self::setup_stack(&mut kernel_stack, entry, kernel_stack_top);
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
            vmm: Vmm::new(),
            kernel_stack_top,
            scratch: PerCpuScratch::new(kernel_stack_top),
            user_entry: 0,
            user_stack: 0,
            heap_start: 0,
            brk: 0,
            kernel_stack,
        }
    }

    fn setup_stack(stack: &mut Vec<u8>, entry: fn() -> !, top: u64) -> u64 {
        let _ = stack;  // retained as a parameter for the unsafe write below
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

    pub fn clone_thread(&self, entry: fn() -> !, priority: u8) -> Task {
        let mut kernel_stack = vec![0u8; KERNEL_STACK_SIZE];
        let top = (kernel_stack.as_mut_ptr() as usize + kernel_stack.len()) as u64 & !15;
        let rsp = Self::setup_stack(&mut kernel_stack, entry, top);

        Task {
            pid: self.pid,
            tid: Tid::next(),
            thread_group: self.thread_group,
            state: TaskState::Ready,
            rsp,
            cr3: self.cr3,
            preempt_count: 0,
            priority,
            parent: self.parent,
            children: Vec::new(),
            exit_code: None,
            vmm: Vmm::new(),
            kernel_stack_top: top,
            scratch: PerCpuScratch::new(top),
            user_entry: 0,
            user_stack: 0,
            heap_start: 0,
            brk: 0,
            kernel_stack,
        }
    }

    pub fn fork_kernel(&self, entry: fn() -> !, priority: u8) -> Task {
        let mut kernel_stack = vec![0u8; KERNEL_STACK_SIZE];
        let top = (kernel_stack.as_mut_ptr() as usize + kernel_stack.len()) as u64 & !15;
        let rsp = Self::setup_stack(&mut kernel_stack, entry, top);
        let pid = Pid::next();
        
        Task {
            pid,
            tid: Tid::next(),
            thread_group: pid,
            state: TaskState::Ready,
            rsp,
            cr3: 0,  // set by SYS_FORK after clone_kernel_half
            preempt_count: 0,
            priority,
            parent: Some(self.pid),
            children: Vec::new(),
            exit_code: None,
            vmm: Vmm::new(),  // set by SYS_FORK after fork_cow
            kernel_stack_top: top,
            scratch: PerCpuScratch::new(top),
            user_entry: 0,
            user_stack: 0,
            heap_start: self.heap_start,
            brk: self.brk,
            kernel_stack,
        }
    }

    pub fn new_user(entry: u64, user_stack: u64, cr3: u64, vmm: Vmm, priority: u8) -> Self {
        let mut kernel_stack = vec![0u8; KERNEL_STACK_SIZE];
        let top = (kernel_stack.as_mut_ptr() as usize + kernel_stack.len()) as u64 & !15;
        let rsp = Self::setup_stack(&mut kernel_stack, user_trampoline, top);
        let pid = Pid::next();

        Task {
            pid,
            tid: Tid::next(),
            thread_group: pid,
            state: TaskState::Ready,
            rsp,
            cr3,
            preempt_count: 0,
            priority,
            parent: None,
            children: Vec::new(),
            exit_code: None,
            vmm,
            kernel_stack_top: top,
            scratch: PerCpuScratch::new(top),
            user_entry: entry,
            user_stack,
            heap_start: 0,
            brk: 0,
            kernel_stack,
        }
    }
}


// Runs in ring 0 on the new task's own stack and address space, then iretq's to ring 3 and never returns.
fn user_trampoline() -> ! {
    let tid = crate::scheduler::current_tid();
    
    let (entry, stack) = {
        let table = TASK_TABLE.lock();
        let task = table.get(tid).expect("user_trampoline: current task missing");
        let guard = task.lock();

        (guard.user_entry, guard.user_stack)
    };

    unsafe { crate::arch::usermode::enter_user_mode(entry, stack) }
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
