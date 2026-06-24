// `syscall`/`sysretq` move between rings on an already-running user thread, but the very first entry has to be forged by hand.
// there is no prior `syscall` to return from.
// build the stack frame `iretq` expects and let it perform the privilege switch for us. 
// `iretq` pops, in order: RIP, CS, RFLAGS, RSP and SS, so we push them in reverse.
// No `swapgs` here: the scheduler already loaded this task's PerCpuScratch into KERNEL_GS_BASE on the context switch, so the user's first `syscall` will `swapgs` the correct kernel stack pointer into place.
pub unsafe fn enter_user_mode(entry: u64, user_stack: u64) -> ! {
    let (user_cs, user_ss) = crate::arch::gdt::user_selectors();
    const USER_RFLAGS: u64 = 0x202;

    unsafe {
        core::arch::asm!(
            "push {ss}",
            "push {rsp_val}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",
            "iretq",
            ss      = in(reg) user_ss as u64,
            rsp_val = in(reg) user_stack,
            rflags  = in(reg) USER_RFLAGS,
            cs      = in(reg) user_cs as u64,
            rip     = in(reg) entry,
            options(noreturn),
        );
    }
}
