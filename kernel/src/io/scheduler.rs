use alloc::vec::Vec;


pub struct DiskRequest {
    pub lba: u64,
    pub count: u64,
    pub is_write: bool,
}


pub trait RequestScheduler {
    fn submit(&mut self, req: DiskRequest);
    fn drain(&mut self) -> Vec<DiskRequest>;
}


// C-SCAN: serves requests from head position upward, then wraps to LBA 0.
pub struct CscanScheduler {
    pending: Vec<DiskRequest>,
    head: u64,
}

impl CscanScheduler {
    pub fn new() -> Self {
        Self { pending: Vec::new(), head: 0 }
    }
}

impl RequestScheduler for CscanScheduler {
    fn submit(&mut self, req: DiskRequest) {
        self.pending.push(req);
    }

    fn drain(&mut self) -> Vec<DiskRequest> {
        let mut batch = core::mem::take(&mut self.pending);
        let h = self.head;

        let mut above: Vec<_> = batch.drain(..).collect();
        above.sort_by_key(|r| r.lba);

        let split = above.iter().position(|r| r.lba >= h).unwrap_or(above.len());
        let below = above.drain(..split).collect::<Vec<_>>();
        above.extend(below);

        if let Some(last) = above.last() {
            self.head = last.lba + last.count;
        }
        above
    }
}


// FCFS with adjacent-write merge: preserves submission order but coalesces consecutive writes to adjacent sectors into one request.
pub struct FcfsScheduler {
    pending: Vec<DiskRequest>,
}

impl FcfsScheduler {
    pub fn new() -> Self {
        Self { pending: Vec::new() }
    }
}

impl RequestScheduler for FcfsScheduler {
    fn submit(&mut self, req: DiskRequest) {
        self.pending.push(req);
    }

    fn drain(&mut self) -> Vec<DiskRequest> {
        let batch = core::mem::take(&mut self.pending);
        let mut merged: Vec<DiskRequest> = Vec::new();

        for req in batch {
            if req.is_write {
                if let Some(last) = merged.last_mut() {
                    if last.is_write && req.lba == last.lba + last.count {
                        last.count += req.count;
                        continue;
                    }
                }
            }
            merged.push(req);
        }

        merged
    }
}
