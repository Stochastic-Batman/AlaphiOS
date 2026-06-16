use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star, SFMask};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;


pub fn init() {
    unsafe { Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS) };

    let (kernel_cs, user_cs) = crate::arch::gdt::syscall_selectors();

    unsafe { Star::write(user_cs, user_cs, kernel_cs, kernel_cs).unwrap(); }

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
