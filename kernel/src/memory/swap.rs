use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Lazy;
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};
use crate::arch::paging::{PageMapper, PHYS_MEM_OFFSET};
use crate::io::disk::{DiskDevice, RamDisk};
use crate::memory::FRAME_ALLOCATOR;
use crate::memory::frame_allocator::FrameAlloc;
use crate::memory::vmm::Vmm;
use crate::process::table::{TaskRegistry, TASK_TABLE};
use crate::sync::spinlock::Spinlock;


const PAGE_SIZE: usize = 4096;
const SECTOR_SIZE: usize = 512;
const SECTORS_PER_PAGE: u64 = (PAGE_SIZE / SECTOR_SIZE) as u64;
const SWAP_PAGES: usize = 1024;
// Reaper keeps the free-frame pool between these bounds.
const POOL_MIN: usize = 64;
const POOL_MAX: usize = 256;


pub trait SwapBackend {
    fn swap_out(&mut self, frame: PhysFrame<Size4KiB>) -> Option<u64>;
    fn swap_in(&mut self, slot: u64, frame: PhysFrame<Size4KiB>);
    fn free_slot(&mut self, slot: u64);
}


pub struct SwapPartition {
    disk: RamDisk,
    free: Vec<bool>,  // true = slot available
    cursor: usize,
}

impl SwapPartition {
    pub fn new(pages: usize) -> Self {
        let disk = RamDisk::new(pages * SECTORS_PER_PAGE as usize, SECTOR_SIZE);
        Self { disk, free: vec![true; pages], cursor: 0 }
    }

    fn alloc_slot(&mut self) -> Option<u64> {
        let n = self.free.len();
        for i in 0..n {
            let idx = (self.cursor + i) % n;
            if self.free[idx] {
                self.free[idx] = false;
                self.cursor = (idx + 1) % n;
                return Some(idx as u64);
            }
        }
        None
    }
}

impl SwapBackend for SwapPartition {
    fn swap_out(&mut self, frame: PhysFrame<Size4KiB>) -> Option<u64> {
        let slot = self.alloc_slot()?;
        let base = (PHYS_MEM_OFFSET + frame.start_address().as_u64()) as *const u8;
        for i in 0..SECTORS_PER_PAGE {
            let off = (i as usize) * SECTOR_SIZE;
            let sector = unsafe { core::slice::from_raw_parts(base.add(off), SECTOR_SIZE) };
            self.disk.write_sector(slot * SECTORS_PER_PAGE + i, sector).ok()?;
        }
        Some(slot)
    }

    fn swap_in(&mut self, slot: u64, frame: PhysFrame<Size4KiB>) {
        let base = (PHYS_MEM_OFFSET + frame.start_address().as_u64()) as *mut u8;
        for i in 0..SECTORS_PER_PAGE {
            let off = (i as usize) * SECTOR_SIZE;
            let sector = unsafe { core::slice::from_raw_parts_mut(base.add(off), SECTOR_SIZE) };
            self.disk.read_sector(slot * SECTORS_PER_PAGE + i, sector).expect("swap_in: read failed");
        }
        self.free_slot(slot);
    }

    fn free_slot(&mut self, slot: u64) {
        if let Some(s) = self.free.get_mut(slot as usize) {
            *s = true;
        }
    }
}


// Initialised eagerly from kernel_main so the RamDisk backing is built while the frame allocator is free, never lazily from inside a lock that already holds it.
static SWAP: Lazy<Spinlock<Option<SwapPartition>>> = Lazy::new(|| Spinlock::new(None));

pub fn init() {
    let part = SwapPartition::new(SWAP_PAGES);
    *SWAP.lock() = Some(part);
}

pub fn swap_in(slot: u64, frame: PhysFrame<Size4KiB>) {
    SWAP.lock().as_mut().expect("swap not initialised").swap_in(slot, frame);
}


// Writes a resident page out to swap, records its slot in the vmm, unmaps it and returns the freed frame to the pool. Caller holds the owning task's lock.
pub fn evict_addr(vmm: &mut Vmm, cr3: u64, vaddr: u64) -> bool {
    let offset = VirtAddr::new(PHYS_MEM_OFFSET);
    let cr3_frame = PhysFrame::containing_address(PhysAddr::new(cr3));
    let mut mapper = unsafe { PageMapper::new_from_frame(cr3_frame, offset) };

    let va = VirtAddr::new(vaddr);
    let Some(phys) = mapper.translate_addr(va) else { return false };
    let flags = match mapper.page_flags(va) {
        Some(f) => f & !(PageTableFlags::ACCESSED | PageTableFlags::DIRTY),
        None => return false,
    };
    let frame = PhysFrame::containing_address(phys);

    let mut fa = FRAME_ALLOCATOR.lock();
    let slot = match SWAP.lock().as_mut().expect("swap not initialised").swap_out(frame) {
        Some(s) => s,
        None => return false,  // swap full
    };

    vmm.record_swapped(va.align_down(PAGE_SIZE as u64).as_u64(), slot, flags);
    let freed = mapper.unmap_page(Page::containing_address(va));
    fa.deallocate(freed);
    true
}


struct Candidate {
    arc: Arc<Spinlock<crate::process::task::Task>>,
    cr3: u64,
    vaddr: u64,
}

// Builds the global victim list, ordered so pages from tasks that hold more than their proportional share are evicted first.
fn gather_candidates() -> Vec<Candidate> {
    let offset = VirtAddr::new(PHYS_MEM_OFFSET);
    let arcs: Vec<_> = {
        let table = TASK_TABLE.lock();
        table.all_tids().into_iter().filter_map(|t| table.get(t)).collect()
    };

    let mut per_task: Vec<(Arc<Spinlock<crate::process::task::Task>>, u64, u64, Vec<u64>)> = Vec::new();
    let mut total_vsize: u64 = 0;

    for arc in arcs {
        let task = arc.lock();
        if task.user_entry == 0 { continue; }
        let cr3 = task.cr3;
        let mapper = unsafe { PageMapper::new_from_frame(PhysFrame::containing_address(PhysAddr::new(cr3)), offset) };

        let mut vsize: u64 = 0;
        let mut resident: Vec<u64> = Vec::new();
        for area in task.vmm.areas() {
            vsize += area.end.as_u64() - area.start.as_u64();
            let mut a = area.start;
            while a < area.end {
                if mapper.translate_addr(a).is_some() {
                    resident.push(a.as_u64());
                }
                a += PAGE_SIZE as u64;
            }
        }
        total_vsize += vsize;
        drop(task);
        per_task.push((arc, cr3, vsize, resident));
    }

    let total_resident: usize = per_task.iter().map(|p| p.3.len()).sum();
    let mut scored: Vec<(i64, Candidate)> = Vec::new();
    for (arc, cr3, vsize, resident) in per_task {
        let share = if total_vsize > 0 {
            (vsize as u128 * total_resident as u128 / total_vsize as u128) as i64
        } else {
            0
        };
        let excess = resident.len() as i64 - share;
        for vaddr in resident {
            scored.push((excess, Candidate { arc: arc.clone(), cr3, vaddr }));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, c)| c).collect()
}

fn read_bits(cr3: u64, vaddr: u64) -> Option<(bool, bool)> {
    let offset = VirtAddr::new(PHYS_MEM_OFFSET);
    let mapper = unsafe { PageMapper::new_from_frame(PhysFrame::containing_address(PhysAddr::new(cr3)), offset) };
    let flags = mapper.page_flags(VirtAddr::new(vaddr))?;
    Some((flags.contains(PageTableFlags::ACCESSED), flags.contains(PageTableFlags::DIRTY)))
}

fn clear_accessed(cr3: u64, vaddr: u64) {
    let offset = VirtAddr::new(PHYS_MEM_OFFSET);
    let mut mapper = unsafe { PageMapper::new_from_frame(PhysFrame::containing_address(PhysAddr::new(cr3)), offset) };
    mapper.clear_accessed(Page::containing_address(VirtAddr::new(vaddr)));
}


// Reaps frames with enhanced second-chance over the proportional victim list. 
// Sweep 0 takes clean unreferenced pages, sweep 1 takes any unreferenced page and clears reference bits, and sweep 2 takes anything to guarantee progress.
pub fn reap_to(target_free: usize) -> usize {
    let mut reaped = 0;

    loop {
        if FRAME_ALLOCATOR.lock().free_count() >= target_free {
            break;
        }

        let candidates = gather_candidates();
        if candidates.is_empty() {
            break;
        }

        let mut progress = false;
        'sweeps: for sweep in 0..3 {
            for c in &candidates {
                let Some((accessed, dirty)) = read_bits(c.cr3, c.vaddr) else { continue };
                let take = match sweep {
                    0 => !accessed && !dirty,
                    1 => !accessed,
                    _ => true,
                };
                if take {
                    let mut task = c.arc.lock();
                    if evict_addr(&mut task.vmm, c.cr3, c.vaddr) {
                        drop(task);
                        reaped += 1;
                        progress = true;
                        if FRAME_ALLOCATOR.lock().free_count() >= target_free {
                            break 'sweeps;
                        }
                    }
                } else if accessed && sweep >= 1 {
                    clear_accessed(c.cr3, c.vaddr);
                }
            }
        }

        if !progress {
            break;
        }
    }

    reaped
}

pub fn reap_if_needed() {
    if FRAME_ALLOCATOR.lock().free_count() < POOL_MIN {
        reap_to(POOL_MAX);
    }
}


// Reaper thread keeps the free-frame pool stocked under memory pressure, then yields.
pub fn reaper_main() -> ! {
    loop {
        reap_if_needed();
        crate::scheduler::schedule();
    }
}
