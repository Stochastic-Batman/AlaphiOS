use crate::fs::fcb::FcbTable;
use crate::fs::fd_table::FdRegistry;
use crate::fs::overlay::{AuthDb, PermsDb, Rwx};
use crate::process::pid::Pid;


#[derive(Debug)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    PermissionDenied,
    Locked,
    IsShared,  // write attempted on immutable-shared file
    NotOpen,
    InvalidPath,
}


pub struct LogicalFs {
    fcbs: FcbTable,
    fds: FdRegistry,
    pub perms: PermsDb,
    pub auth: AuthDb,
}

impl LogicalFs {
    pub fn new() -> Self {
        Self {
            fcbs: FcbTable::new(),
            fds: FdRegistry::new(),
            perms: PermsDb::new(),
            auth: AuthDb::new(),
        }
    }

    // fatfs stores names via LFN.
    // We must reject opens whose name differs only in case from an existing entry.
    // Because we have no directory cache yet, we check the FCB table which holds every currently-open path.
    // A full implementation scans the fatfs directory. 
    // The FCB check is the kernel-enforced layer that matters at the syscall boundary.
    fn case_conflict(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        // For now this is a placeholder: real impl iterates fcbs.map.keys().
        let _ = lower;
        false  // TODO: iterate FCB keys and compare lowercased
    }

    // --- Permission check helper (Decision 92) ---
    fn check_perm(&self, path: &str, pid: Pid, want_write: bool) -> Result<(), FsError> {
        let Some(entry) = self.perms.get(path) else { return Ok(()) };
        if want_write && !entry.world_rwx.can_write() {
            return Err(FsError::PermissionDenied);
        }
        if !want_write && !entry.world_rwx.can_read() {
            return Err(FsError::PermissionDenied);
        }
        Ok(())
    }


    pub fn open(&mut self, path: &str, pid: Pid) -> Result<u64, FsError> {
        if self.case_conflict(path) {
            return Err(FsError::AlreadyExists);
        }

        self.check_perm(path, pid, false)?;
        let fcb = self.fcbs.open(path);

        // Mandatory exclusive lock
        if fcb.locked_by.is_some() {
            return Err(FsError::Locked);
        }
       
        fcb.locked_by = Some(pid);
        fcb.open_count += 1;
        let fd = self.fds.table_for(pid).alloc(path.into());
        Ok(fd)
    }

    pub fn close(&mut self, fd: u64, pid: Pid) -> Result<(), FsError> {
        let entry = self.fds.table_for(pid).remove(fd).ok_or(FsError::NotOpen)?;
        let path = entry.path.clone();

        if let Some(fcb) = self.fcbs.get_mut(&path) {
            fcb.open_count = fcb.open_count.saturating_sub(1);
            if fcb.locked_by == Some(pid) {
                fcb.locked_by = None;
            }
            if fcb.open_count == 0 {
                self.fcbs.remove(&path);
            }
        }
        Ok(())
    }

    pub fn read(&mut self, fd: u64, pid: Pid, buf: &mut [u8]) -> Result<usize, FsError> {
        let path = self.fds.table_for(pid).get(fd).map(|e| e.path.clone()).ok_or(FsError::NotOpen)?;
        self.check_perm(&path, pid, false)?;

        let fcb = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?;
        // Actual read deferred to the FAT layer via the cursor.
        // cursor advances here; fatfs call happens in the FS syscall handler
        // which has access to the FatFs<D> instance.
        let _ = (buf, fcb);

        Ok(0)  // placeholder; wired in dispatch.rs
    }

    pub fn write(&mut self, fd: u64, pid: Pid, buf: &[u8]) -> Result<usize, FsError> {
        let path = self.fds.table_for(pid).get(fd).map(|e| e.path.clone()).ok_or(FsError::NotOpen)?;

        // Reject writes to immutable-shared files before lock check.
        {
            let fcb = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?;
            if fcb.is_shared {
                return Err(FsError::IsShared);
            }
        }
        self.check_perm(&path, pid, true)?;

        let fcb = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?;
        fcb.dirty = true;
        let _ = buf;

        Ok(0)  // placeholder
    }

    pub fn seek(&mut self, fd: u64, pid: Pid, offset: u64) -> Result<(), FsError> {
        let path = self.fds.table_for(pid).get(fd).map(|e| e.path.clone()).ok_or(FsError::NotOpen)?;
        let fcb = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?;
        fcb.cursor = offset;
        Ok(())
    }

    pub fn create(&mut self, path: &str, pid: Pid) -> Result<u64, FsError> {
        if self.case_conflict(path) {
            return Err(FsError::AlreadyExists);
        }
    
        let fcb = self.fcbs.open(path);
        if fcb.locked_by.is_some() {
            return Err(FsError::Locked);
        }

        fcb.locked_by = Some(pid);
        fcb.open_count += 1;
        Ok(self.fds.table_for(pid).alloc(path.into()))
    }

    pub fn delete(&mut self, path: &str) -> Result<(), FsError> {
        // Immutable-shared files cannot be deleted, and their name cannot be reused after deletion
        if let Some(fcb) = self.fcbs.get_mut(path) {
            if fcb.is_shared {
                return Err(FsError::IsShared);
            }
        }
        self.fcbs.remove(path);
        self.perms.remove(path);
        Ok(())
    }

    pub fn mark_shared(&mut self, path: &str) -> Result<(), FsError> {
        let fcb = self.fcbs.get_mut(path).ok_or(FsError::NotFound)?;
        fcb.is_shared = true;
        Ok(())
    }

    pub fn chmod(&mut self, path: &str, owner: u8, group: u8, world: u8) -> Result<(), FsError> {
        if let Some(entry) = self.perms.get_mut(path) {
            entry.owner_rwx = Rwx(owner);
            entry.group_rwx = Rwx(group);
            entry.world_rwx = Rwx(world);
        }
        Ok(())
    }

    pub fn verify_password(&self, uid: u32, password: &[u8]) -> bool {
        self.auth.verify(uid, password)
    }

    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), FsError> {
        if self.case_conflict(new) { return Err(FsError::AlreadyExists); }
        if let Some(fcb) = self.fcbs.get_mut(old) {
            if fcb.is_shared { return Err(FsError::IsShared); }
        }
        Ok(())
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), FsError> {
        if self.case_conflict(path) { return Err(FsError::AlreadyExists); }
        // fatfs: root_dir().create_dir(path)
        Ok(())
    }

    pub fn rmdir(&mut self, path: &str) -> Result<(), FsError> {
        // fatfs: root_dir().remove(path)
        Ok(())
    }
}
