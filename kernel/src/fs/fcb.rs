use alloc::collections::BTreeMap;
use alloc::string::String;
use crate::process::pid::Pid;


pub struct Fcb {
    pub path: String,
    pub cursor: u64,
    pub open_count: u32,
    pub dirty: bool,
    pub locked_by: Option<Pid>,
    pub is_shared: bool,   // immutable once true
}

pub struct FcbTable {
    map: BTreeMap<String, Fcb>,
}

impl FcbTable {
    pub fn new() -> Self {
        Self { map: BTreeMap::new() }
    }

    pub fn open(&mut self, path: &str) -> &mut Fcb {
        self.map.entry(path.into()).or_insert_with(|| Fcb {
            path: path.into(),
            cursor: 0,
            open_count: 0,
            dirty: false,
            locked_by: None,
            is_shared: false,
        })
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut Fcb> {
        self.map.get_mut(path)
    }

    pub fn remove(&mut self, path: &str) {
        self.map.remove(path);
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(|s| s.as_str())
    }
}
