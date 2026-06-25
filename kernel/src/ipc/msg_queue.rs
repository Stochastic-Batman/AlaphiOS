use alloc::collections::VecDeque;
use core::cmp::min;
use crate::sync::spinlock::Spinlock;
use crate::sync::condvar::CondVar;
use super::channel::{IpcChannel, IpcError};


#[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
pub const MSG_SIZE: usize = 256;
#[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
pub const MAX_MESSAGES: usize = 128;


#[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
pub struct MsgQueue {
    queue: Spinlock<VecDeque<[u8; MSG_SIZE]>>,
    not_empty: CondVar,
    not_full: CondVar,
    closed: Spinlock<bool>,
}

impl MsgQueue {
    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub const fn new() -> Self {
        Self {
            queue: Spinlock::new(VecDeque::new()),
            not_empty: CondVar::new(),
            not_full: CondVar::new(),
            closed: Spinlock::new(false),
        }
    }
}


impl IpcChannel for MsgQueue {
    fn send(&self, data: &[u8]) -> Result<(), IpcError> {
        if data.len() > MSG_SIZE {
            return Err(IpcError::TooLarge);
        }

        // Zero-pad the trailing bytes if the payload is smaller than MSG_SIZE
        let mut msg = [0u8; MSG_SIZE];
        msg[..data.len()].copy_from_slice(data);

        let mut guard = self.queue.lock();

        loop {
            if *self.closed.lock() {
                return Err(IpcError::Closed);
            }

            if guard.len() < MAX_MESSAGES {
                guard.push_back(msg);
                self.not_empty.signal();
                return Ok(());
            }

            guard = self.not_full.wait(guard);
        }
    }

    fn recv(&self, out: &mut [u8]) -> Result<usize, IpcError> {
        let mut guard = self.queue.lock();

        loop {
            if let Some(msg) = guard.pop_front() {
                let to_read = min(out.len(), MSG_SIZE);
                out[..to_read].copy_from_slice(&msg[..to_read]);
                
                self.not_full.signal();
                
                return Ok(to_read);
            }

            if *self.closed.lock() {
                return Ok(0);
            }

            guard = self.not_empty.wait(guard);
        }
    }

    fn close(&self) {
        let _queue_guard = self.queue.lock();
        *self.closed.lock() = true;
        self.not_empty.broadcast();
        self.not_full.broadcast();
    }
}
