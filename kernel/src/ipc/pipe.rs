use alloc::collections::VecDeque;
use core::cmp::min; 
use crate::sync::spinlock::Spinlock;
use crate::sync::condvar::CondVar;
use super::channel::{IpcChannel, IpcError};


const PIPE_BUF: usize = 4096;


pub struct Pipe {
    buf: Spinlock<VecDeque<u8>>,
    not_empty: CondVar,
    not_full:  CondVar,
    closed: Spinlock<bool>,
}

impl Pipe {
    pub const fn new() -> Self { 
        Self {  // should be zero-initialized
            buf: Spinlock::new(VecDeque::new()),
            not_empty: CondVar::new(),
            not_full: CondVar::new(),
            closed: Spinlock::new(false),
        }
    }
}


impl IpcChannel for Pipe {
    fn send(&self, data: &[u8]) -> Result<(), IpcError> {
        let mut guard = self.buf.lock();
        let mut bytes_written = 0;

        while bytes_written < data.len() {
            if *self.closed.lock() {
                return Err(IpcError::Closed);
            }

            let available_space = PIPE_BUF - guard.len();

            if available_space == 0 {
                guard = self.not_full.wait(guard);
                continue;
            }

            let to_write = min(data.len() - bytes_written, available_space);
            for i in 0..to_write {
                guard.push_back(data[bytes_written + i]);
            }
            bytes_written += to_write;

            self.not_empty.signal();
        }

        Ok(())
    }

    fn recv(&self, out: &mut [u8]) -> Result<usize, IpcError> {
        let mut guard = self.buf.lock();

        while guard.is_empty() {
            if *self.closed.lock() {
                return Ok(0);
            }

            guard = self.not_empty.wait(guard);
        }

        let to_read = min(out.len(), guard.len());

        for i in 0..to_read {
            if let Some(byte) = guard.pop_front() {
                out[i] = byte;
            }
        }

        if to_read > 0 {
            self.not_full.signal();
        }

        Ok(to_read)
    }

    fn close(&self) {
        let _buf_guard = self.buf.lock();
        *self.closed.lock() = true;
        self.not_empty.broadcast();
        self.not_full.broadcast();
    }
}
