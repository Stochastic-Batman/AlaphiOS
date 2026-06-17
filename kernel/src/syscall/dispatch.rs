use crate::process::lifecycle;
use crate::process::pid::Pid;
use crate::scheduler;
use crate::syscall::numbers::*;


const ENOSYS: isize = -38;


// Hardware saves RIP -> RCX, RFLAGS -> R11 and swaps CS/SS.
// Syscall ABI: nr=RAX, args=RDI RSI RDX R10 R8 R9
// C call ABI:  args=RDI RSI RDX RCX R8 R9
// Only mismatch is arg3: move R10 -> RCX before the call.
#[unsafe(no_mangle)]
pub extern "C" fn syscall_handler(nr: usize, arg0: usize, _arg1: usize, _arg2: usize, _arg3: usize, _arg4: usize,) -> isize {
    match nr {
        SYS_GETPID  => scheduler::current_pid().as_u64() as isize,
        SYS_GETPPID => scheduler::current_ppid().map(|p| p.as_u64() as isize).unwrap_or(-1),
        SYS_GETTID  => scheduler::current_tid().as_u64() as isize,
        SYS_YIELD   => { scheduler::schedule(); 0 }
        SYS_EXIT    => lifecycle::exit(arg0 as i32),
        SYS_WAITPID => lifecycle::wait(Pid::from_u64(arg0 as u64)) as isize,
        _           => ENOSYS,
    }
}
