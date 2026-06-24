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
        "test rdi, rdi",
        "jz 1f",
        "mov [{rsp_off} + rdi], rsp",
        "1:",
        "mov rsp, [{rsp_off} + rsi]",
        "lea rax, [rsi + {scratch_off}]",
        "mov rdx, rax",
        "shr rdx, 32",
        "mov ecx, 0xC0000102",
        "wrmsr",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "ret",
        rsp_off = const core::mem::offset_of!(Task, rsp),
        scratch_off = const core::mem::offset_of!(Task, scratch),
    )
}
