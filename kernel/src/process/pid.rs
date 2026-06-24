use core::sync::atomic::{AtomicU64, Ordering};


/// Newtype so a Pid can never be accidentally passed where a plain u64 is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pid(u64);

/// thread ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tid(u64);


/// Global counter. Starts at 1; PID 0 is reserved (idle task).
static NEXT_PID: AtomicU64 = AtomicU64::new(1);
static NEXT_TID: AtomicU64 = AtomicU64::new(1);


impl Pid {
    pub fn next() -> Self {
        // the counter only needs to be unique, not synchronized with any other memory operation.
        let id: u64 = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        Self(id)
    }

    pub fn from_u64(v: u64) -> Self { Self(v) }
    pub fn as_u64(self) -> u64 { self.0 }
}

impl Tid {
    pub fn next() -> Self {
        let id: u64 = NEXT_TID.fetch_add(1, Ordering::Relaxed);
        Self(id)
    }

    pub fn from_u64(v: u64) -> Self { Self(v) }
    pub fn as_u64(self) -> u64 { self.0 }
}
