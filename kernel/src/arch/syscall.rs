use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;


// GS_BASE (after swapgs) points at one of these per-task.
// switch_to() repoints KERNEL_GS_BASE at the incoming task's instance on every context switch, so syscall_entry always finds the right kernel stack regardless of which task is "current."
#[repr(C)]
pub struct PerCpuScratch {
    pub user_rsp_scratch: u64,  // offset 0: syscall_entry stashes user RSP
    pub kernel_rsp: u64,        // offset 8: top of this task's kernel stack
}

impl PerCpuScratch {
    pub const fn new(kernel_stack_top: u64) -> Self {
        Self { 
            user_rsp_scratch: 0, 
            kernel_rsp: kernel_stack_top 
        }
    }
}


// Register state captured on syscall entry, in push order.
// Lets a child task resume as if returning from this exact syscall, with its own copy of every register the parent had.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,  // user RFLAGS, saved by hardware into r11 on syscall
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,  // user RIP, saved by hardware into rcx on syscall
    pub rax: u64,  // syscall number on entry; overwritten with return value on exit
    pub user_rsp: u64,
}


pub fn init() {
    unsafe { Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS) };

    let (kernel_cs, user_cs) = crate::arch::gdt::syscall_selectors();

    unsafe {
        // STAR MSR Address: 0xC0000081
        // bits 32-47: Kernel Segment Base (CS is this, DS/SS is this + 8)
        // bits 48-63: User Segment Base (CS is this + 16, SS is this + 8)
        // adjust the base inputs to satisfy how hardware parses the bit fields:
        let kernel_base = kernel_cs.0;
        let user_base = (user_cs.0 - 16) | 3; // shift back so User CS hits target mapping

        let msr_val = ((user_base as u64) << 48) | ((kernel_base as u64) << 32);
        
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000081u32,
            in("eax") (msr_val & 0xFFFF_FFFF) as u32,
            in("edx") (msr_val >> 32) as u32,
        );
    }

    LStar::write(VirtAddr::new(syscall_entry as u64));

    // mask IF on entry so we don't take a timer interrupt before saving user state.
    SFMask::write(RFlags::INTERRUPT_FLAG);
}


// The code below was written by Claude Sonnet 4.6
// Hardware saves RIP -> RCX, RFLAGS -> R11 and swaps CS/SS. User RSP is NOT
// saved by hardware -- we capture it ourselves into the per-task scratch
// slot immediately after swapgs, before RSP is repointed at the kernel stack.
//
// Syscall ABI: nr=RAX, args=RDI RSI RDX R10 R8 R9
// C call ABI:  args=RDI RSI RDX RCX R8 R9
// Only mismatch is arg3 (R10 vs RCX), handled below by reshuffling after
// the trap frame is saved (RCX is needed as scratch before that point is
// safe, since RCX itself is live user state we must not clobber early).
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "swapgs",

        // gs:[0] = user_rsp_scratch, gs:[8] = kernel_rsp (see PerCpuScratch)
        "mov gs:[0], rsp",
        "mov rsp, gs:[8]",

        // Save full trap frame, in TrapFrame field order (top of struct = last pushed = lowest address).
        "push qword ptr gs:[0]",  // user_rsp
        "push rax",               // rax (syscall nr on entry)
        "push rcx",                // rcx (user rip)
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",                // r11 (user rflags)
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov rdi, rax",  // syscall number
        "mov rsi, rsp",  // &TrapFrame
        "call syscall_handler",

        "mov [rsp + 112], rax",  // store return value into TrapFrame.rax
        "mov rdi, rsp",
        "call deliver_pending_signal",

        // Restore, in reverse push order.
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "pop rsp",

        "swapgs",
        "sysretq",
    )
}
