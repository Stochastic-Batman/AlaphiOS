// Grouped by the chapter of "Operating System Concepts" that motivates them.

// Chapter 3: Processes 
pub const SYS_FORK: usize = 0;
pub const SYS_EXIT: usize = 1;
pub const SYS_WAITPID: usize = 2;
pub const SYS_GETPID: usize = 3;
pub const SYS_GETPPID: usize = 4;

// Chapter 4: Threads 
pub const SYS_CLONE: usize = 5;   // create thread (CLONE_VM | CLONE_FILES | CLONE_SIGHAND)
pub const SYS_GETTID: usize = 6;
pub const SYS_JOIN: usize = 7;   // join() semantics 

// Chapter 5: CPU Scheduling
pub const SYS_YIELD: usize = 8;

// Chapters 6 & 7: Synchronisation 
pub const SYS_MUTEX_LOCK: usize = 9;
pub const SYS_MUTEX_TRYLOCK: usize = 10;  // non-blocking trylock() 
pub const SYS_MUTEX_UNLOCK: usize = 11;
pub const SYS_CONDVAR_WAIT: usize = 12;
pub const SYS_CONDVAR_SIGNAL: usize = 13;
pub const SYS_CONDVAR_BROADCAST: usize = 14;

// Chapter 8: Deadlocks 
pub const SYS_TRYLOCK: usize = 15;  // generic trylock exposed for IPC handles 
pub const SYS_GETRESOURCE: usize = 16;  // stable resource handle lookup

// Chapter 9 & 10: Memory  
pub const SYS_BRK: usize = 17;  // grow/shrink heap (demand-paged)
pub const SYS_MPROTECT: usize = 18;  // change VmArea flags

// Chapter 12: I/O
pub const SYS_READ: usize = 19;
pub const SYS_WRITE: usize = 20;
pub const SYS_IOCTL: usize = 21;
pub const SYS_OPEN_DEV: usize = 22;  // open a character or block device by name
pub const SYS_CLOSE_DEV: usize = 23;

// Chapter 13 & 14: File System 
pub const SYS_OPEN: usize = 24;
pub const SYS_CLOSE: usize = 25;
pub const SYS_SEEK: usize = 26;
pub const SYS_CREATE: usize = 27;
pub const SYS_DELETE: usize = 28;
pub const SYS_TRUNCATE: usize = 29;
pub const SYS_RENAME: usize = 30;
pub const SYS_MKDIR: usize = 31;
pub const SYS_RMDIR: usize = 32;  // recursive delete 
pub const SYS_GETDENTS: usize = 33;  // read directory entries
pub const SYS_STAT: usize = 34;  // file metadata 
pub const SYS_CHMOD: usize = 35;  // rwx permission bits 
pub const SYS_CHOWN: usize = 36;  // uid/gid ownership 
pub const SYS_FLOCK: usize = 37;  // mandatory exclusive lock 
pub const SYS_FUNLOCK: usize = 38;
pub const SYS_MARK_SHARED: usize = 39;  // make file immutably shared 

// Chapter 9: IPC 
pub const SYS_MSG_SEND: usize = 40;  // fixed-size indirect message passing 
pub const SYS_MSG_RECV: usize = 41;
pub const SYS_SHM_OPEN: usize = 42;  // shared memory, POSIX-like 
pub const SYS_SHM_CLOSE: usize = 43;
pub const SYS_PIPE_CREATE: usize = 44;  // unidirectional pipe 
pub const SYS_PIPE_READ: usize = 45;
pub const SYS_PIPE_WRITE: usize = 46;
pub const SYS_PIPE_CLOSE: usize = 47;

// Chapter 16 & 17: Security & Protection 
pub const SYS_LOGIN: usize = 48;  // authenticate against auth.db 
pub const SYS_LOGOUT: usize = 49;
pub const SYS_ACCESS: usize = 50;  // access(i,j) on the Access Matrix 
pub const SYS_GRANT: usize = 51;  // copy/owner/control operations 
pub const SYS_REVOKE: usize = 52;

// Process signal / cancellation 
pub const SYS_KILL: usize = 53;  // send signal to task (deferred cancellation)
pub const SYS_SIGRETURN: usize = 54;  // return from signal handler
pub const SYS_SIGNAL: usize = 55;  // register signal handler
pub const SYSCALL_COUNT: usize = 56;
