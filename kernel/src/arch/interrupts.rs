use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::instructions::port::Port;
use crate::sync::atomic::KernelAtomicUsize;


const PIC1_OFFSET: u8 = 0x20;
const PIC2_OFFSET: u8 = PIC1_OFFSET + 8;


static PICS: Mutex<ChainedPics> = unsafe { Mutex::new(ChainedPics::new(PIC1_OFFSET, PIC2_OFFSET)) };
pub static TICKS: KernelAtomicUsize = KernelAtomicUsize::new(0);


// Ring buffer for raw scancodes deposited by the keyboard FLIH.
// Plain array because this runs before the heap exists.
static SCANCODE_BUF: Mutex<[u8; 256]> = Mutex::new([0u8; 256]);
static SCANCODE_HEAD: KernelAtomicUsize = KernelAtomicUsize::new(0);
static SCANCODE_TAIL: KernelAtomicUsize = KernelAtomicUsize::new(0);


pub fn init() {
    unsafe {
        PICS.lock().initialize();
    }
    x86_64::instructions::interrupts::enable();
}


pub fn tick() {
    TICKS.increment();
}


pub fn end_of_interrupt(vector: u8) {
    unsafe {
        PICS.lock().notify_end_of_interrupt(vector);
    }
}


pub fn push_scancode(scancode: u8) {
    let tail = SCANCODE_TAIL.load();
    let next_tail = (tail + 1) % 256;
    if next_tail != SCANCODE_HEAD.load() {
        SCANCODE_BUF.lock()[tail] = scancode;
        SCANCODE_TAIL.store(next_tail);
    }
    // If the buffer is full the scancode is silently dropped.
    // At 256 slots this only happens if the SLIH falls far behind.
}


pub fn pop_scancode() -> Option<u8> {
    let head = SCANCODE_HEAD.load();
    if head == SCANCODE_TAIL.load() {
        return None;
    }
    let scancode = SCANCODE_BUF.lock()[head];
    SCANCODE_HEAD.store((head + 1) % 256);
    Some(scancode)
}
