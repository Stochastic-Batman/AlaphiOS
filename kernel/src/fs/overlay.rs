use alloc::collections::BTreeMap;
use alloc::string::String;
use sha2::{Digest, Sha256};


// rwx bits packed as u8: bit 2 = read, bit 1 = write, bit 0 = execute.
#[derive(Clone, Copy)]
pub struct Rwx(pub u8);

impl Rwx {
    pub fn can_read(self) -> bool { self.0 & 0b100 != 0 }
    pub fn can_write(self) -> bool { self.0 & 0b010 != 0 }
    pub fn can_execute(self) -> bool { self.0 & 0b001 != 0 }
}


#[derive(Clone)]
pub struct PermEntry {
    pub uid: u32,
    pub gid: u32,
    pub owner_rwx: Rwx,
    pub group_rwx: Rwx,
    pub world_rwx: Rwx,
}


// In-memory permission table, keyed by absolute path.
pub struct PermsDb {
    entries: BTreeMap<String, PermEntry>,
}

impl PermsDb {
    pub fn new() -> Self {
        Self { entries: BTreeMap::new() }
    }

    pub fn get(&self, path: &str) -> Option<&PermEntry> {
        self.entries.get(path)
    }

    pub fn set(&mut self, path: String, entry: PermEntry) {
        self.entries.insert(path, entry);
    }

    pub fn remove(&mut self, path: &str) {
        self.entries.remove(path);
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut PermEntry> {
        self.entries.get_mut(path)
    }
}

pub struct AuthEntry {
    pub uid: u32,
    pub salt: [u8; 16],
    pub hash: [u8; 32],  // salt concatenated password
}

impl AuthEntry {
    pub fn new(uid: u32, password: &[u8]) -> Self {
        let salt = [0u8; 16];
        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(password);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Self { uid, salt, hash }
    }
}

pub struct AuthDb {
    entries: BTreeMap<u32, AuthEntry>,
}

impl AuthDb {
    pub fn new() -> Self {
        Self { entries: BTreeMap::new() }
    }

    pub fn verify(&self, uid: u32, password: &[u8]) -> bool {
        let Some(entry) = self.entries.get(&uid) else { return false };

        let mut hasher = Sha256::new();
        hasher.update(entry.salt);
        hasher.update(password);
        
        let result = hasher.finalize();
        result.as_slice() == entry.hash
    }

    pub fn insert(&mut self, entry: AuthEntry) {
        self.entries.insert(entry.uid, entry);
    }
}
