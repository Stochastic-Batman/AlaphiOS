// flat-binary user-program loader.

// flat binary has no headers: the bytes ARE the code, linked to run at a fixed virtual base.
// build a fresh address space, eagerly map the code (its bytes must physically exist before the first instruction fetch), and hand the stack to the demand-pager via a single anonymous VmArea - only stack pages that get touched ever cost a frame.
use alloc::vec;
use x86_64::VirtAddr;
use x86_64::structures::paging::{Page, PageTableFlags};
use crate::arch::paging::{PageMapper, PHYS_MEM_OFFSET};
use crate::memory::FRAME_ALLOCATOR;
use crate::memory::frame_allocator::FrameAlloc;
use crate::memory::vmm::{VmArea, VmAreaKind, Vmm};
use crate::process::task::Task;

pub const USER_CODE_BASE: u64 = 0x40_0000;  // 4 MiB; classic flat-binary load address.
pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000; // top of the canonical lower (user) half, page-aligned.
pub const USER_STACK_SIZE: u64 = 64 * 1024;

const PAGE_SIZE: usize = 4096;


pub fn load_flat(bytes: &[u8], priority: u8) -> Task {
    let phys_offset = VirtAddr::new(PHYS_MEM_OFFSET);
    let parent_mapper = unsafe { PageMapper::new(phys_offset) };

    // Fresh address space: kernel half copied in, user half empty.
    let (l4_frame, mut child_mapper) = {
        let mut fa = FRAME_ALLOCATOR.lock();
        parent_mapper.clone_kernel_half(&mut *fa)
    };

    // Code is mapped RX for ring 3: USER_ACCESSIBLE set, WRITABLE cleared.
    // USER_ACCESSIBLE on the leaf also propagates to the parent tables map_to creates, so ring 3 can reach these pages.
    {
        let mut fa = FRAME_ALLOCATOR.lock();
        let code_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        let mut off = 0usize;
        while off < bytes.len() {
            let frame = fa.allocate().expect("loader: OOM mapping user code");
            let dst = (PHYS_MEM_OFFSET + frame.start_address().as_u64()) as *mut u8;
            let n = core::cmp::min(bytes.len() - off, PAGE_SIZE);
            unsafe {
                core::ptr::write_bytes(dst, 0, PAGE_SIZE);
                core::ptr::copy_nonoverlapping(bytes.as_ptr().add(off), dst, n);
            }
            let page = Page::containing_address(VirtAddr::new(USER_CODE_BASE + off as u64));
            child_mapper.map_page(page, frame, code_flags, &mut *fa);
            off += PAGE_SIZE;
        }
    }

    let heap_start = (USER_CODE_BASE + bytes.len() as u64 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1);

    let mut vmm = Vmm::new();
    vmm.add_area(VmArea {
        start: VirtAddr::new(USER_STACK_TOP - USER_STACK_SIZE),
        end:   VirtAddr::new(USER_STACK_TOP),
        flags: PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
        kind:  VmAreaKind::Anonymous,
    });

    let mut task = Task::new_user(USER_CODE_BASE, USER_STACK_TOP, l4_frame.start_address().as_u64(), vmm, priority);
    task.heap_start = heap_start;
    task.brk = heap_start;
    task
}


pub fn load_path(path: &str, priority: u8) -> Task {
    let pid = crate::scheduler::current_pid();
    let mut fs = crate::fs::FS.lock();

    let size = fs.stat(path).expect("load_path: stat failed").size as usize;
    assert!(size > 0, "load_path: file is empty");

    let fd = fs.open(path, pid, 0, 0).expect("load_path: open failed");
    let mut buf = vec![0u8; size];
    let n = fs.read(fd, pid, 0, 0, &mut buf).expect("load_path: read failed");
    fs.close(fd, pid).expect("load_path: close failed");

    assert_eq!(n, size, "load_path: short read");
    load_flat(&buf, priority)
}
