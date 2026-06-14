// Manages a contiguous arena of physical memory using power-of-two block sizes.
// Free lists are intrusive: the first bytes of each free block store a pointer to the next free block at that ord, so there is zero external metadata.
use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use crate::sync::spinlock::Spinlock;


pub const MAX_ORDER: usize = 18;
pub const MIN_BLOCK_SIZE: usize = 32; 


pub struct BuddyAllocator {
    // free_lists[i] is the head of a linked list of free blocks of size MIN_BLOCK_SIZE * 2^i bytes.
    free_lists: [*mut u8; MAX_ORDER + 1],
    heap_start: usize,
    heap_size: usize,
}


// SAFETY: We wrap BuddyAllocator in a Spinlock before exposing it globally.
unsafe impl Send for BuddyAllocator {}


impl BuddyAllocator {
    pub const fn empty() -> Self {
        Self {
            free_lists: [ptr::null_mut(); MAX_ORDER + 1],
            heap_start: 0,
            heap_size: 0,
        }
    }


    // SAFETY: The entire range must be mapped, writable, and not in use elsewhere.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_size = heap_size;

        let mut curr = heap_start;
        let end = heap_start + heap_size;

        while curr < end {
            let mut ord = MAX_ORDER;
            while ord > 0 {
                let size = MIN_BLOCK_SIZE << ord;
                if curr + size <= end && (curr & (size - 1)) == 0 { break; }
                ord -= 1;
            }
        
            let size = MIN_BLOCK_SIZE << ord;
            if curr + size > end { break; }

            unsafe { self.add_to_free_list(curr as *mut u8, ord); }
            curr += size;
        }
    }


    unsafe fn alloc_inner(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(layout.align()).max(MIN_BLOCK_SIZE);

        let mut ord = 0;
        while ord < MAX_ORDER && (MIN_BLOCK_SIZE << ord) < size { ord += 1; }

        if (MIN_BLOCK_SIZE << ord) < size { return ptr::null_mut(); }

        let mut found_ord = ord;
        while found_ord <= MAX_ORDER && self.free_lists[found_ord].is_null() { found_ord += 1; }

        if found_ord > MAX_ORDER { return ptr::null_mut(); }

        let block = self.free_lists[found_ord];
        self.free_lists[found_ord] = unsafe { ptr::read(block as *const *mut u8) };

        while found_ord > ord {
            found_ord -= 1;
            let buddy = unsafe { block.add(MIN_BLOCK_SIZE << found_ord) };
            unsafe { self.add_to_free_list(buddy, found_ord); }
        }

        block
    }


    unsafe fn dealloc_inner(&mut self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().max(layout.align()).max(MIN_BLOCK_SIZE);
        
        let mut ord = 0;
        while ord < MAX_ORDER && (MIN_BLOCK_SIZE << ord) < size { ord += 1; }

        let mut current_ptr = ptr as usize;

        while ord < MAX_ORDER {
            let buddy = current_ptr ^ (MIN_BLOCK_SIZE << ord);
            let mut is_free = false;
            let mut ptr_to_node = &mut self.free_lists[ord] as *mut *mut u8;

            while unsafe { !(*ptr_to_node).is_null() } {
                if unsafe { *ptr_to_node } as usize == buddy {
                    let next_node = unsafe { ptr::read(*ptr_to_node as *const *mut u8) };
                    unsafe { ptr::write(ptr_to_node, next_node); }
                    is_free = true;
                    break;
                }
                ptr_to_node = unsafe { *ptr_to_node as *mut *mut u8 };
            }

            if is_free {
                current_ptr = current_ptr.min(buddy);
                ord += 1;
            } else {
                break;
            }
        }

        unsafe { self.add_to_free_list(current_ptr as *mut u8, ord); }
    }


    unsafe fn add_to_free_list(&mut self, ptr: *mut u8, order: usize) {
        unsafe { ptr::write(ptr as *mut *mut u8, self.free_lists[order]); }
        self.free_lists[order] = ptr;
    }
}


// GlobalAlloc wrapper
// The kernel's global allocator. Box/Vec/Arc all route through here.
pub struct LockedBuddyAllocator(Spinlock<BuddyAllocator>);


impl LockedBuddyAllocator {
    pub const fn new() -> Self {
        LockedBuddyAllocator(Spinlock::new(BuddyAllocator::empty()))
    }

    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        unsafe { self.0.lock().init(heap_start, heap_size); }
    }
}


unsafe impl GlobalAlloc for LockedBuddyAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.0.lock().alloc_inner(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.0.lock().dealloc_inner(ptr, layout); }
    }
}
