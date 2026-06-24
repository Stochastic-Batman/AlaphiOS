use crate::process::task::Task;


// Both pointers must be valid and non-null, and the caller owns synchronisation.
// CR3 is loaded only after RSP points at the incoming kernel stack, which is mapped in every address space and so survives the switch.
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
        "mov rax, [{cr3_off} + rsi]",
        "test rax, rax",
        "jz 2f",
        "mov rcx, cr3",
        "cmp rax, rcx",
        "je 2f",
        "mov cr3, rax",
        "2:",
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
        cr3_off = const core::mem::offset_of!(Task, cr3),
        scratch_off = const core::mem::offset_of!(Task, scratch),
    )
}
