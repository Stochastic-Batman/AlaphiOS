// file metadata exposed to user space via SYS_STAT.
// fixed C-like layout so it can be written directly through a raw user pointer, matching the raw-pointer ABI style used by SYS_READ/SYS_WRITE.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stat {
    pub size: u64,
    pub is_dir: u8,
    pub owner_rwx: u8,
    pub group_rwx: u8,
    pub world_rwx: u8,
    pub uid: u32,
    pub gid: u32,
}


impl Stat {
    // SAFETY: caller guarantees `dst` points to at least size_of::<Stat>() writable, correctly aligned bytes in the calling task's address space.
    pub unsafe fn write_to_user(&self, dst: *mut u8) {
        let src = self as *const Stat as *const u8;
        unsafe {
            core::ptr::copy_nonoverlapping(src, dst, core::mem::size_of::<Stat>());
        }
    }
}
