use crate::arch::interrupts::pop_scancode;
use crate::io::device::CharDevice;
use spin::Lazy;
use crate::sync::spinlock::Spinlock;

#[rustfmt::skip]
const SCANCODE_TO_ASCII: [u8; 58] = [
    0,    0,    b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8',  // 0x00-0x09
    b'9', b'0', b'-', b'=', 8,    b'\t',                          // 0x0A-0x0F
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p',  // 0x10-0x19
    b'[', b']', b'\n', 0,                                          // 0x1A-0x1D
    b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l',        // 0x1E-0x26
    b';', b'\'', b'`', 0,   b'\\',                                // 0x27-0x2B
    b'z', b'x', b'c', b'v', b'b', b'n', b'm',                    // 0x2C-0x32
    b',', b'.', b'/', 0,    0,    0,    b' ',                     // 0x33-0x39
];


pub struct Console {
    line_buf: [u8; 256],
    line_len: usize,
    ready_buf: [u8; 256],
    ready_len: usize,
    ready_pos: usize,
}

impl Console {
    pub const fn new() -> Self {
        Self {
            line_buf: [0; 256],
            line_len: 0,
            ready_buf: [0; 256],
            ready_len: 0,
            ready_pos: 0,
        }
    }

    fn decode_scancode(code: u8) -> Option<u8> {
        if code & 0x80 != 0 { return None; }
        SCANCODE_TO_ASCII.get(code as usize).copied().filter(|&c| c != 0)
    }

    fn drain_scancodes(&mut self) {
        while let Some(code) = pop_scancode() {
            if let Some(ch) = Self::decode_scancode(code) {
                if self.line_len < self.line_buf.len() {
                    self.line_buf[self.line_len] = ch;
                    self.line_len += 1;
                    crate::SERIAL.lock().send(ch);
                }
                if ch == b'\n' {
                    self.ready_buf[..self.line_len].copy_from_slice(&self.line_buf[..self.line_len]);
                    self.ready_len = self.line_len;
                    self.ready_pos = 0;
                    self.line_len = 0;
                }
            }
        }
    }
}

impl CharDevice for Console {
    fn get(&mut self) -> Option<u8> {
        if self.ready_pos < self.ready_len {
            let ch = self.ready_buf[self.ready_pos];
            self.ready_pos += 1;
            return Some(ch);
        }
        self.drain_scancodes();
        if self.ready_pos < self.ready_len {
            let ch = self.ready_buf[self.ready_pos];
            self.ready_pos += 1;
            Some(ch)
        } else {
            None
        }
    }

    fn put(&mut self, byte: u8) {
        crate::SERIAL.lock().send(byte);
    }
}

pub static CONSOLE: Lazy<Spinlock<Console>> = Lazy::new(|| Spinlock::new(Console::new()));
