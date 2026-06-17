use x86_64::structures::paging::{PhysFrame, Size4KiB};


pub trait SwapBackend {
    fn swap_in(&mut self, slot: u64, frame: PhysFrame<Size4KiB>);
    fn swap_out(&mut self, frame: PhysFrame<Size4KiB>) -> u64;  // returns slot id
}

pub struct NullSwap;

impl SwapBackend for NullSwap {
    fn swap_in(&mut self, _slot: u64, _frame: PhysFrame<Size4KiB>) {
        panic!("swap_in: swap not implemented");
    }

    fn swap_out(&mut self, _frame: PhysFrame<Size4KiB>) -> u64 {
        panic!("swap_out: swap not implemented");
    }
}
