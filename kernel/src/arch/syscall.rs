use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;


pub fn init() {
    unsafe { Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS) };

    let (kernel_cs, user_cs) = crate::arch::gdt::syscall_selectors();

    unsafe {
        // STAR MSR Address: 0xC0000081
        // Bits 32-47: Kernel Segment Base (CS is this, DS/SS is this + 8)
        // Bits 48-63: User Segment Base (CS is this + 16, SS is this + 8)
        // We adjust the base inputs to satisfy how hardware parses the bit fields:
        let kernel_base = kernel_cs.0;
        let user_base = (user_cs.0 - 16) | 3; // Shift back so User CS hits target mapping

        let msr_val = ((user_base as u64) << 48) | ((kernel_base as u64) << 32);
        
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC0000081u32,
            in("eax") (msr_val & 0xFFFF_FFFF) as u32,
            in("edx") (msr_val >> 32) as u32,
        );
    }

    LStar::write(VirtAddr::new(syscall_entry as u64));

    // Mask IF on entry so we don't take a timer interrupt before saving user state.
    SFMask::write(RFlags::INTERRUPT_FLAG);
}


// Hardware saves RIP -> RCX, RFLAGS -> R11 and swaps CS/SS.
// Syscall ABI: nr=RAX, args=RDI RSI RDX R10 R8 R9
// C call ABI:  args=RDI RSI RDX RCX R8 R9
// Only mismatch is arg3: move R10 -> RCX before the call.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "swapgs",
        "mov rcx, r10",
        "mov rdi, rax",
        "call syscall_handler",
        "swapgs",
        "sysretq",
    )
}
