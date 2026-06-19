pub mod fat;
pub mod fcb;
pub mod fd_table;
pub mod overlay;
pub mod vfs;

use spin::Lazy;
use crate::sync::spinlock::Spinlock;
use vfs::LogicalFs;

pub static FS: Lazy<Spinlock<LogicalFs>> = Lazy::new(|| Spinlock::new(LogicalFs::new()));
