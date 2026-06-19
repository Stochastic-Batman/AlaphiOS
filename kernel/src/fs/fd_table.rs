use alloc::collections::BTreeMap;
use alloc::string::String;
use crate::process::pid::Pid;


// Per-process open-file table entry
pub struct FdEntry {
    pub path: String,
}

// Per-process table: fd -> FdEntry
pub struct ProcFdTable {
    next_fd: u64,
    map: BTreeMap<u64, FdEntry>,
}

impl ProcFdTable {
    pub fn new() -> Self {
        Self { next_fd: 3, map: BTreeMap::new() }  // 0, 1 & 2 reserved for stdin, stdout & stderr respectively
    }

    pub fn alloc(&mut self, path: String) -> u64 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.map.insert(fd, FdEntry { path });
        fd
    }

    pub fn get(&self, fd: u64) -> Option<&FdEntry> {
        self.map.get(&fd)
    }

    pub fn remove(&mut self, fd: u64) -> Option<FdEntry> {
        self.map.remove(&fd)
    }
}


// Global per-process table registry.
pub struct FdRegistry {
    tables: BTreeMap<Pid, ProcFdTable>,
}

impl FdRegistry {
    pub fn new() -> Self {
        Self { tables: BTreeMap::new() }
    }

    pub fn table_for(&mut self, pid: Pid) -> &mut ProcFdTable {
        self.tables.entry(pid).or_insert_with(ProcFdTable::new)
    }

    pub fn remove_process(&mut self, pid: Pid) {
        self.tables.remove(&pid);
    }
}
