// Thin newtype over core's AtomicUsize so the rest of the kernel never imports core::sync::atomic directly. 
// On x86-64, core's atomics already emit LOCK-prefixed instructions.
use core::sync::atomic::{AtomicUsize, Ordering};


pub struct KernelAtomicUsize(AtomicUsize);


// using Ordering::SeqCst for conservative correctness; can be relaxed later if profiling shows contention.
impl KernelAtomicUsize {
    pub const fn new(val: usize) -> Self {
        Self(AtomicUsize::new(val))
    }

    pub fn load(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    pub fn store(&self, val: usize) {
        self.0.store(val, Ordering::SeqCst);
    }

    pub fn fetch_add(&self, val: usize) -> usize {
        self.0.fetch_add(val, Ordering::SeqCst)
    }

    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub fn fetch_sub(&self, val: usize) -> usize {
        self.0.fetch_sub(val, Ordering::SeqCst)
    }

    pub fn increment(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub fn decrement(&self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }

    #[allow(dead_code)]  // not used currently, but I believe this should be present for completeness.
    pub fn compare_exchange(&self, expected: usize, new: usize) -> Result<usize, usize> {
        self.0.compare_exchange(expected, new, Ordering::Acquire, Ordering::Relaxed)
    }
}
