use alloc::string::String;
use alloc::vec::Vec;
use fatfs::{Read, Seek, SeekFrom, Write};
use crate::fs::fat::FatFs;
use crate::fs::fcb::FcbTable;
use crate::fs::fd_table::FdRegistry;
use crate::fs::overlay::{AuthDb, PermsDb, Rwx};
use crate::fs::stat::Stat;
use crate::io::disk::RamDisk;
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
    NotADirectory,
    IsADirectory,
    NotMounted,
}


pub struct LogicalFs {
    fcbs: FcbTable,
    fds: FdRegistry,
    pub perms: PermsDb,
    pub auth: AuthDb,
    disk: Option<FatFs<RamDisk>>,
}

impl LogicalFs {
    pub fn new() -> Self {
        Self {
            fcbs: FcbTable::new(),
            fds: FdRegistry::new(),
            perms: PermsDb::new(),
            auth: AuthDb::new(),
            disk: None,
        }
    }

    // called once from kernel_main after the boot RamDisk has been formatted and mounted. 
    // everything before this point (create/open/etc. called pre-mount) returns FsError::NotMounted.
    pub fn mount_disk(&mut self, disk: FatFs<RamDisk>) {
        self.disk = Some(disk);
    }

    pub fn disk(&self) -> Option<&FatFs<RamDisk>> {
        self.disk.as_ref()
    }

    // boot loader needs simultaneous &disk and &mut auth/&mut perms. 
    pub fn disk_and_overlays_mut(&mut self) -> Option<(&FatFs<RamDisk>, &mut AuthDb, &mut PermsDb)> {
        let disk = self.disk.as_ref()?;
        Some((disk, &mut self.auth, &mut self.perms))
    }

    fn disk_mut(&mut self) -> Result<&mut FatFs<RamDisk>, FsError> {
        self.disk.as_mut().ok_or(FsError::NotMounted)
    }

    fn to_fat_path(path: &str) -> Result<&str, FsError> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidPath);
        }
        Ok(path.trim_start_matches('/'))
    }

    // fatfs stores names via LFN. 
    // reject opens/creates whose name differs only in case from an existing open FCB entry. 
    // this only catches conflicts among currently-open files.
    fn case_conflict(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        self.fcbs.paths().any(|p| p != path && p.to_lowercase() == lower)
    }

    fn check_perm(&self, path: &str, uid: u32, gid: u32, want_write: bool) -> Result<(), FsError> {
        if uid == 0 { return Ok(()); }
        let Some(entry) = self.perms.get(path) else { return Ok(()) };
        let rwx = if uid == entry.uid {
            entry.owner_rwx
        } else if gid == entry.gid {
            entry.group_rwx
        } else {
            entry.world_rwx
        };
        if want_write && !rwx.can_write() {
            return Err(FsError::PermissionDenied);
        }
        if !want_write && !rwx.can_read() {
            return Err(FsError::PermissionDenied);
        }
        Ok(())
    }


    pub fn open(&mut self, path: &str, pid: Pid, uid: u32, gid: u32) -> Result<u64, FsError> {
        if self.case_conflict(path) {
            return Err(FsError::AlreadyExists);
        }

        self.check_perm(path, uid, gid, false)?;

        let fat_path = Self::to_fat_path(path)?;
        {
            let disk = self.disk_mut()?;
            disk.root_dir().open_file(fat_path).map_err(|_| FsError::NotFound)?;
        }

        let fcb = self.fcbs.open(path);
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

    pub fn read(&mut self, fd: u64, pid: Pid, uid: u32, gid: u32, buf: &mut [u8]) -> Result<usize, FsError> {
        let path = self.fds.table_for(pid).get(fd).map(|e| e.path.clone()).ok_or(FsError::NotOpen)?;
        self.check_perm(&path, uid, gid, false)?;

        let cursor = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?.cursor;
        let fat_path = Self::to_fat_path(&path)?;

        let n = {
            let disk = self.disk_mut()?;
            let mut file = disk.root_dir().open_file(fat_path).map_err(|_| FsError::NotFound)?;
            file.seek(SeekFrom::Start(cursor)).map_err(|_| FsError::InvalidPath)?;
            file.read(buf).map_err(|_| FsError::InvalidPath)?
        };

        self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?.cursor = cursor + n as u64;
        Ok(n)
    }

    pub fn write(&mut self, fd: u64, pid: Pid, uid: u32, gid: u32, buf: &[u8]) -> Result<usize, FsError> {
        let path = self.fds.table_for(pid).get(fd).map(|e| e.path.clone()).ok_or(FsError::NotOpen)?;

        {
            let fcb = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?;
            if fcb.is_shared {
                return Err(FsError::IsShared);
            }
        }
        self.check_perm(&path, uid, gid, true)?;

        let cursor = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?.cursor;
        let fat_path = Self::to_fat_path(&path)?;

        {
            let disk = self.disk_mut()?;
            let mut file = disk.root_dir().open_file(fat_path).map_err(|_| FsError::NotFound)?;
            file.seek(SeekFrom::Start(cursor)).map_err(|_| FsError::InvalidPath)?;
            file.write_all(buf).map_err(|_| FsError::InvalidPath)?;
        }

        let fcb = self.fcbs.get_mut(&path).ok_or(FsError::NotOpen)?;
        fcb.cursor = cursor + buf.len() as u64;
        fcb.dirty = true;
        Ok(buf.len())
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

        let fat_path = Self::to_fat_path(path)?;
        {
            let disk = self.disk_mut()?;
            disk.root_dir().create_file(fat_path).map_err(|_| FsError::InvalidPath)?;
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
        if let Some(fcb) = self.fcbs.get_mut(path) {
            if fcb.is_shared {
                return Err(FsError::IsShared);
            }
        }

        let fat_path = Self::to_fat_path(path)?;
        let disk = self.disk_mut()?;
        disk.root_dir().remove(fat_path).map_err(|_| FsError::NotFound)?;

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
        if self.case_conflict(new) {
            return Err(FsError::AlreadyExists);
        }
        if let Some(fcb) = self.fcbs.get_mut(old) {
            if fcb.is_shared {
                return Err(FsError::IsShared);
            }
        }

        let old_fat = Self::to_fat_path(old)?;
        let new_fat = Self::to_fat_path(new)?;
        let disk = self.disk_mut()?;
        let root = disk.root_dir();
        root.rename(old_fat, &root, new_fat).map_err(|_| FsError::NotFound)?;

        Ok(())
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), FsError> {
        if self.case_conflict(path) {
            return Err(FsError::AlreadyExists);
        }

        let fat_path = Self::to_fat_path(path)?;
        let disk = self.disk_mut()?;
        disk.root_dir().create_dir(fat_path).map_err(|_| FsError::AlreadyExists)?;
        Ok(())
    }

    pub fn rmdir(&mut self, path: &str) -> Result<(), FsError> {
        // deleting a non-empty directory recursively deletes its contents. 
        // fatfs::Dir::remove refuses non-empty directories, so we recurse manually first.
        let fat_path = Self::to_fat_path(path)?;
        self.remove_dir_recursive(fat_path)
    }

    fn remove_dir_recursive(&mut self, fat_path: &str) -> Result<(), FsError> {
        let entries: Vec<(String, bool)> = {
            let disk = self.disk_mut()?;
            let dir = disk.root_dir().open_dir(fat_path).map_err(|_| FsError::NotFound)?;
            dir.iter()
                .filter_map(|r| r.ok())
                .map(|e| (e.file_name(), e.is_dir()))
                .filter(|(name, _)| name != "." && name != "..")
                .collect()
        };

        for (name, is_dir) in entries {
            let mut child_path = String::from(fat_path);
            if !child_path.is_empty() {
                child_path.push('/');
            }
            child_path.push_str(&name);

            if is_dir {
                self.remove_dir_recursive(&child_path)?;
            } else {
                let disk = self.disk_mut()?;
                disk.root_dir().remove(&child_path).map_err(|_| FsError::NotFound)?;
            }
        }

        let disk = self.disk_mut()?;
        disk.root_dir().remove(fat_path).map_err(|_| FsError::NotFound)?;
        Ok(())
    }

    // directory listing backing SYS_GETDENTS.
    pub fn getdents(&mut self, path: &str) -> Result<Vec<(String, bool)>, FsError> {
        let fat_path = Self::to_fat_path(path)?;
        let disk = self.disk_mut()?;
        let dir = disk.root_dir().open_dir(fat_path).map_err(|_| FsError::NotFound)?;

        Ok(dir.iter()
            .filter_map(|r| r.ok())
            .map(|e| (e.file_name(), e.is_dir()))
            .filter(|(name, _)| name != "." && name != "..")
            .collect())
    }

    // file metadata. Size and is_dir come from fatfs.
    // ownership/permission bits come from the kernel-managed overlay, defaulting to "no restriction recorded" when absent.
    pub fn stat(&mut self, path: &str) -> Result<Stat, FsError> {
        let fat_path = Self::to_fat_path(path)?;

        let (size, is_dir) = {
            let disk = self.disk_mut()?;
            let root = disk.root_dir();

            if let Ok(mut file) = root.open_file(fat_path) {
                let size = file.seek(SeekFrom::End(0)).map_err(|_| FsError::InvalidPath)?;
                (size, false)
            } else if root.open_dir(fat_path).is_ok() {
                (0, true)
            } else {
                return Err(FsError::NotFound);
            }
        };

        let mut s = Stat { size, is_dir: is_dir as u8, ..Default::default() };
        if let Some(entry) = self.perms.get(path) {
            s.uid = entry.uid;
            s.gid = entry.gid;
            s.owner_rwx = entry.owner_rwx.0;
            s.group_rwx = entry.group_rwx.0;
            s.world_rwx = entry.world_rwx.0;
        }
        Ok(s)
    }
}
