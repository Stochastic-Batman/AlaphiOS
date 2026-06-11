// preemptive kernel; 
// spinlocks must disable preemption while held to prevent a higher-priority task
// from re-entering kernel code that already holds this lock.
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::sync::atomic::KernelAtomicUsize; // Stub until task struct exists. Will be replaced by current_task().preempt_count.increment() / .decrement().


static GLOBAL_PREEMPT_COUNT: KernelAtomicUsize = KernelAtomicUsize::new(0);


pub fn preempt_disable() {
    GLOBAL_PREEMPT_COUNT.increment();
}

pub fn preempt_enable() {
    // After task struct is implemented: also check if a reschedule is pending and yield if so.
    GLOBAL_PREEMPT_COUNT.decrement();
}


pub struct Spinlock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}


pub struct SpinlockGuard<'a, T> {
    lock: &'a Spinlock<T>,
}


// SAFETY: Spinlock provides mutual exclusion; T must be Send for cross-thread use.
unsafe impl<T: Send> Sync for Spinlock<T> {}
unsafe impl<T: Send> Send for Spinlock<T> {}


impl<T> Spinlock<T> {
    pub const fn new(val: T) -> Self {
        Self {
            locked: AtomicBool::new(false), 
            data: UnsafeCell::new(val),
        }
    }

    // Spin until the lock is acquired, then disable preemption.
    // On x86-64, the PAUSE instruction inside the spin loop reduces memory bus contention and speeds up lock release detection.
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        while self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        preempt_disable();
        SpinlockGuard::new(self)
    }

    // Single attempt; returns None immediately if the lock is taken.
    // Used by trylock() syscall surface.
    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            preempt_disable();
            Some(SpinlockGuard::new(self))
        } else {
            None
        }
    }

    // SAFETY: caller must guarantee no other reference exists.
    // Only used during single-threaded init before scheduling starts.
    pub unsafe fn get_mut_unchecked(&self) -> &mut T {
        &mut *self.data.get()
    }
}



impl<'a, T> SpinlockGuard<'a, T> {
    fn new(lock: &'a Spinlock<T>) -> Self {
        Self { lock }
    }
}


impl<T> Deref for SpinlockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: guard existence proves we hold the lock.
        unsafe { &*self.lock.data.get() }
    }
}


impl<T> DerefMut for SpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: guard existence and exclusive reference to the guard (&mut self)
        unsafe { &mut *self.lock.data.get() }
    }
}


impl<T> Drop for SpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        preempt_enable();
    }
}
