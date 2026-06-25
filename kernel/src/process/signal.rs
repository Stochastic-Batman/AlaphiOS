use crate::arch::syscall::TrapFrame;
use crate::process::table::{TASK_TABLE, TaskRegistry};


pub const MAX_SIGNALS: usize = 32;
#[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
pub const SIGTERM: u8 = 15;
const SYS_SIGRETURN_NR: u8 = 54;
const TRAMPOLINE_PAD: u64 = 16;


static TRAMPOLINE: [u8; 10] = [
    0xB8, SYS_SIGRETURN_NR, 0x00, 0x00, 0x00,
    0x0F, 0x05,
    0xEB, 0xFE,
    0x90,
];


pub struct SignalState {
    pub pending: u32,
    pub handlers: [u64; MAX_SIGNALS],
}

impl SignalState {
    pub fn new() -> Self {
        Self { pending: 0, handlers: [0; MAX_SIGNALS] }
    }

    pub fn set_pending(&mut self, signum: u8) {
        if (signum as usize) < MAX_SIGNALS {
            self.pending |= 1 << signum;
        }
    }

    pub fn clear_pending(&mut self, signum: u8) {
        if (signum as usize) < MAX_SIGNALS {
            self.pending &= !(1 << signum);
        }
    }

    pub fn set_handler(&mut self, signum: u8, handler: u64) {
        if (signum as usize) < MAX_SIGNALS {
            self.handlers[signum as usize] = handler;
        }
    }

    pub fn next_pending(&self) -> Option<(u8, u64)> {
        if self.pending == 0 { return None; }
        let signum = self.pending.trailing_zeros() as u8;
        Some((signum, self.handlers[signum as usize]))
    }
}


#[unsafe(no_mangle)]
pub extern "C" fn deliver_pending_signal(frame: &mut TrapFrame) {
    let tid = crate::scheduler::current_tid();

    let (signum, handler) = {
        let table = TASK_TABLE.lock();
        let task_arc = match table.get(tid) {
            Some(a) => a,
            None => return,
        };
        let mut task = task_arc.lock();

        let (signum, handler) = match task.signals.next_pending() {
            Some(s) => s,
            None => return,
        };
        task.signals.clear_pending(signum);
        (signum, handler)
    };

    if handler == 0 {
        crate::process::lifecycle::exit(-(signum as i32));
    }

    let old_rsp = frame.user_rsp;
    let frame_size = core::mem::size_of::<TrapFrame>() as u64;
    let new_rsp = (old_rsp - frame_size - TRAMPOLINE_PAD - 8) & !0xF;

    let tramp_addr = new_rsp + 8;
    let saved_addr = new_rsp + 8 + TRAMPOLINE_PAD;

    unsafe {
        core::ptr::write(saved_addr as *mut TrapFrame, *frame);

        core::ptr::copy_nonoverlapping(
            TRAMPOLINE.as_ptr(),
            tramp_addr as *mut u8,
            TRAMPOLINE.len(),
        );
        core::ptr::write_bytes(
            (tramp_addr as *mut u8).add(TRAMPOLINE.len()),
            0x90,
            (TRAMPOLINE_PAD as usize) - TRAMPOLINE.len(),
        );

        core::ptr::write(new_rsp as *mut u64, tramp_addr);
    }

    frame.rcx = handler;
    frame.user_rsp = new_rsp;
    frame.rdi = signum as u64;
}
