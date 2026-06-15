use crate::process::task::Task;


// Both pointers must be valid and non-null. Caller owns synchronisation.
#[unsafe(naked)]
pub unsafe extern "C" fn switch_to(outgoing: *mut Task, incoming: *const Task) {
    core::arch::naked_asm!(
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov [rdi + {rsp_off}], rsp",
        "mov rsp, [rsi + {rsp_off}]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "ret",
        rsp_off = const core::mem::offset_of!(Task, rsp),
    )
}
