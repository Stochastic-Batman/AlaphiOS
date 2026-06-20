// kernel-managed /.system/ domain.
// auth.db and perms.db use fixed-size binary records.
// Both files are read directly through fatfs at boot, bypassing LogicalFs entirely.
// normal user syscalls never reach this module.
use crate::fs::fat::FatFs;
use crate::fs::overlay::{AuthDb, AuthEntry, PermEntry, PermsDb, Rwx};
use crate::io::disk::DiskDevice;
use crate::serial_println;
use fatfs::{Seek, Write};


pub const AUTH_DB_PATH: &str = ".system/auth.db";
pub const PERMS_DB_PATH: &str = ".system/perms.db";
const PATH_MAX: usize = 64;
const AUTH_RECORD_SIZE: usize = 1 + 4 + 16 + 32;  // status(1) + uid(4) + salt(16) + hash(32) = 53 bytes
const PERMS_RECORD_SIZE: usize = 1 + PATH_MAX + 4 + 4 + 1 + 1 + 1;  // status(1) + path(64, zero-padded) + uid(4) + gid(4) + owner(1) + group(1) + world(1) = 76 bytes
const STATUS_CLEAN: u8 = 0;
const STATUS_DIRTY: u8 = 1;


fn encode_auth_record(buf: &mut [u8; AUTH_RECORD_SIZE], status: u8, entry: &AuthEntry) {
    buf[0] = status;
    buf[1..5].copy_from_slice(&entry.uid.to_le_bytes());
    buf[5..21].copy_from_slice(&entry.salt);
    buf[21..53].copy_from_slice(&entry.hash);
}


fn decode_auth_record(buf: &[u8; AUTH_RECORD_SIZE]) -> (u8, AuthEntry) {
    let status = buf[0];
    let uid = u32::from_le_bytes(buf[1..5].try_into().unwrap());
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&buf[5..21]);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&buf[21..53]);
    (status, AuthEntry { uid, salt, hash })
}


fn encode_perms_record(buf: &mut [u8; PERMS_RECORD_SIZE], status: u8, path: &str, entry: &PermEntry) {
    buf[0] = status;

    let path_bytes = path.as_bytes();
    let n = path_bytes.len().min(PATH_MAX);
    buf[1..1 + n].copy_from_slice(&path_bytes[..n]);
    for b in &mut buf[1 + n..1 + PATH_MAX] {
        *b = 0;
    }

    let off = 1 + PATH_MAX;
    buf[off..off + 4].copy_from_slice(&entry.uid.to_le_bytes());
    buf[off + 4..off + 8].copy_from_slice(&entry.gid.to_le_bytes());
    buf[off + 8] = entry.owner_rwx.0;
    buf[off + 9] = entry.group_rwx.0;
    buf[off + 10] = entry.world_rwx.0;
}


fn decode_perms_record(buf: &[u8; PERMS_RECORD_SIZE]) -> (u8, alloc::string::String, PermEntry) {
    let status = buf[0];

    let path_raw = &buf[1..1 + PATH_MAX];
    let path_len = path_raw.iter().position(|&b| b == 0).unwrap_or(PATH_MAX);
    let path = core::str::from_utf8(&path_raw[..path_len]).unwrap_or("").into();

    let off = 1 + PATH_MAX;
    let uid = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
    let gid = u32::from_le_bytes(buf[off + 4..off + 8].try_into().unwrap());
    let entry = PermEntry {
        uid,
        gid,
        owner_rwx: Rwx(buf[off + 8]),
        group_rwx: Rwx(buf[off + 9]),
        world_rwx: Rwx(buf[off + 10]),
    };

    (status, path, entry)
}

fn check_and_clear_status(status: u8, label: &str) {
    if status == STATUS_DIRTY {
        serial_println!("/.system/{}: dirty status bit on boot, clearing (no repair performed)", label);
    }
}


// Called once from kernel_main, after fs::mount_boot_disk().
// Missing files are not an error: a freshly formatted volume has neither, and both BTreeMaps simply start empty.
pub fn load_at_boot<D: DiskDevice>(disk: &FatFs<D>, auth: &mut AuthDb, perms: &mut PermsDb) {
    load_auth_db(disk, auth);
    load_perms_db(disk, perms);
}


fn load_auth_db<D: DiskDevice>(disk: &FatFs<D>, auth: &mut AuthDb) {
    let root = disk.root_dir();
    let mut file = match root.open_file(AUTH_DB_PATH) {
        Ok(f) => f,
        Err(_) => {
            serial_println!("auth.db not present, starting with empty auth table");
            return;
        }
    };

    let mut buf = [0u8; AUTH_RECORD_SIZE];
    loop {
        match read_exact_or_eof(&mut file, &mut buf) {
            Ok(true) => {
                let (status, entry) = decode_auth_record(&buf);
                check_and_clear_status(status, "auth.db");
                auth.insert(entry);
            }
            Ok(false) => break,
            Err(_) => {
                serial_println!("auth.db: read error, stopping load early");
                break;
            }
        }
    }
}


fn load_perms_db<D: DiskDevice>(disk: &FatFs<D>, perms: &mut PermsDb) {
    let root = disk.root_dir();
    let mut file = match root.open_file(PERMS_DB_PATH) {
        Ok(f) => f,
        Err(_) => {
            serial_println!("perms.db not present, starting with empty perms table");
            return;
        }
    };

    let mut buf = [0u8; PERMS_RECORD_SIZE];
    loop {
        match read_exact_or_eof(&mut file, &mut buf) {
            Ok(true) => {
                let (status, path, entry) = decode_perms_record(&buf);
                check_and_clear_status(status, "perms.db");
                perms.set(path, entry);
            }
            Ok(false) => break,
            Err(_) => {
                serial_println!("perms.db: read error, stopping load early");
                break;
            }
        }
    }
}


fn read_exact_or_eof<F: fatfs::Read>(file: &mut F, buf: &mut [u8]) -> Result<bool, ()> {
    let mut got = 0;

    while got < buf.len() {
        let n = file.read(&mut buf[got..]).map_err(|_| ())?;
        
        if n == 0 {
            return if got == 0 { Ok(false) } else { Err(()) };
        }
    
        got += n;
    }

    Ok(true)
}

pub fn append_auth_entry<D: DiskDevice>(disk: &FatFs<D>, entry: &AuthEntry) -> Result<(), ()> {
    let root = disk.root_dir();
    let mut file = root.create_file(AUTH_DB_PATH).map_err(|_| ())?;
    file.seek(fatfs::SeekFrom::End(0)).map_err(|_| ())?;
    let mut buf = [0u8; AUTH_RECORD_SIZE];
    encode_auth_record(&mut buf, STATUS_CLEAN, entry);
    file.write_all(&buf).map_err(|_| ())
}


pub fn append_perms_entry<D: DiskDevice>(disk: &FatFs<D>, path: &str, entry: &PermEntry) -> Result<(), ()> {
    let root = disk.root_dir();
    let mut file = root.create_file(PERMS_DB_PATH).map_err(|_| ())?;
    file.seek(fatfs::SeekFrom::End(0)).map_err(|_| ())?;

    let mut buf = [0u8; PERMS_RECORD_SIZE];
    encode_perms_record(&mut buf, STATUS_CLEAN, path, entry);
    file.write_all(&buf).map_err(|_| ())
}
