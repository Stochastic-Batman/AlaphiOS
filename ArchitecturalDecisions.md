**Development environment: WSL2 on an HP EliteBook (Intel x86-64). AlaphiOS is compiled using Rust 1.96.0 / Cargo 1.96.0. The operating system is tested exclusively through QEMU x86-64 emulation; physical hardware deployment is out of scope for this project.**

Use as many abstractions (interfaces/traits) as possible so that if a decision is made to switch some algorithm later, there is only a single file to modify (for example, a `SecondaryStorageAllocation` interface, whose concrete implementation is `IndexedAllocation`).

The architectural decisions below do not necessarily optimize the operating system for any specific purpose; they are chosen because the approach is beautiful or challenging (or both). Some time-consuming (time to write the code, not OS performance-wise) optimizations are skipped. Networking is also skipped, because testing is infamously time-consuming; although it would be challenging, other things (besides the OS) are preferred during the summer as well.

If any details are not specified below, follow the Linux defaults unless implementing the detail would be too time-consuming.


## Chapters 1, 2, & 3: Introduction; Operating-System Structures; Processes

1. The operating system is designed for desktop and laptop computers, not for mobile devices, servers, or embedded systems.
2. The Rust standard library (`std`) is not used; the kernel is built as a `#![no_std]` binary.
3. The system handles keyboard I/O and disk I/O, utilizing a terminal-like text interface instead of a graphical user interface (GUI).
4. A minimal set of system calls is defined to fully support a multi-threaded execution environment.
5. A monolithic kernel architecture is adopted; no runtime module loading mechanism is provided.
6. The kernel is loaded using the Rust `bootloader` crate, which generates a bootable disk image directly consumable by QEMU without requiring a GRUB2 configuration. The `bootloader_api` crate provides the entry-point contract and the `BootInfo` structure that delivers the physical memory map, framebuffer descriptor, and physical-memory offset into the kernel at startup.
7. AlaphiOS targets QEMU x86-64 emulation exclusively as its runtime and test environment. QEMU was created by the legendary Fabrice Bellard and is the standard tool for iterative hobby OS development due to its built-in GDB stub (`-s -S` flags), `-monitor` console, snapshot-and-restore, instant soft reboot, and full guest memory inspection, all without risking physical hardware.
8. A dedicated Process Control Block (PCB) structure is defined to manage process states and attributes.
9. Interprocess communication (IPC) exposes a unified abstraction layer, providing concrete implementations for both message passing (fixed-size, indirect communication) and shared memory (POSIX-like API with a bounded buffer).
10. Ordinary pipes are implemented as unidirectional communication channels.
11. Networking sockets are omitted from the design layout.


## Chapter 4: Threads & Concurrency

12. Guard pages are implemented at the boundary of thread stacks to detect and prevent stack overflows.
13. The user-to-kernel thread relationship strictly follows the one-to-one mapping model.
14. Thread execution supports asynchronous thread creation, alongside `fork()` and `join()` semantics, aligning with standard POSIX thread behaviors.
15. Thread pools are not managed by the operating system kernel; they are implemented in user space by application programmers.
16. Thread signaling supports both synchronous and asynchronous delivery, utilizing deferred cancellation exclusively.
17. The kernel adopts the Linux paradigm of using a unified `task` abstraction instead of distinguishing between threads and processes, grouping tasks via thread groups to handle `getpid()` and evaluating flags such as `CLONE_VM` and `CLONE_SIGHAND`. `CLONE_FILES` (shared file descriptor table across threads) is omitted; each task maintains its own descriptor space.
18. The kernel is fully preemptive, using synchronization primitives such as mutex locks to prevent race conditions on shared kernel data structures.


## Chapter 5: CPU Scheduling

19. CPU scheduling is governed by a single-core Multilevel Feedback Queue (MLFQ) scheduler where queue 0 represents the highest priority, and queues containing threads of equal priority use Round-Robin (RR) scheduling. Symmetric Multi-Processing (SMP) is not implemented; a single global scheduler instance operates on one processor core.
20. Thread scheduling implements system-contention scope (SCS), mapping user-level threads directly to kernel-schedulable entities.


## Chapters 6 & 7: Synchronization

21. Monitors are constructed using a combination of a spinlock and a condition variable accompanied by a thread wait queue.
22. A global `preempt_count` atomic variable increments when a spinlock is acquired or `preempt_disable()` is called, and decrements when a spinlock is released or `preempt_enable()` is called. If the hardware timer interrupt fires while `preempt_count > 0`, preemption is bypassed to protect the core from deadlocking on nested kernel resources. A per-task field exists for future migration but the global counter is authoritative.
23. An architecture-specific atomic integer module is implemented utilizing x86 `LOCK` prefix hardware primitives to perform non-blocking, indivisible operations on shared metrics - such as pipe offsets, shared `task` counters, or PID allocators - without triggering scheduler context switches.


## Chapter 8: Deadlocks

24. A strict, statically defined global lock hierarchy is enforced for all internal kernel spinlocks to eliminate the circular-wait condition.
25. Non-blocking `trylock()` variants are exposed via the system call API for monitors and IPC mechanisms to allow user-space threads to safely back off if a lock is unavailable.
26. System calls provide transparent access to stable process identifiers and unique resource handles, enabling application software to implement total lock ordering and randomized backoff protocols.


## Chapter 9: Main Memory

27. Program loading uses a flat-binary format linked to a fixed virtual base address. Dynamic loading of program components at runtime is not supported.
28. Code compilation relies on static linking; dynamic link libraries (DLLs) are not supported.
29. The operating system kernel resides in high memory addresses, while user spaces occupy lower memory address spaces.
30. Memory allocation is managed through paging rather than contiguous memory allocation.
31. Swapping is used to manage processes when available physical memory cannot satisfy the demands of an arriving process.
32. Physical memory is divided into fixed-size blocks to completely eliminate external fragmentation, ignoring any internal fragmentation that occurs.
33. The page and frame size is hardcoded to 4 KB.
34. The Translation Lookaside Buffer (TLB) is manually flushed during every context switch when a new page table is selected.
35. A valid-invalid bit is maintained in each page table entry to signify memory presence.
36. Page-table origin and page-table length constraints are checked to restrict processes to their designated user address spaces.
37. Explicit shared-memory page mapping between unrelated processes is not supported. Copy-on-write fork temporarily shares physical frames between parent and child until a write fault triggers duplication.
38. Address translation is handled through a multi-level hierarchical (forward-mapped) page table structure.
39. The system supports swapping at the page level rather than at the whole-process level.


## Chapter 10: Virtual Memory

40. Virtual address spaces are structured as sparse layouts with a gap separating the stack and heap regions.
41. The user space stack grows downward toward lower addresses, while the heap grows upward toward higher addresses.
42. Demand paging is utilized to load pages into memory only when they are accessed.
43. Allocation of free frames is handled via zero-fill-on-demand.
44. Pages are initially demand-paged directly from the file system and are subsequently written to swap space when they are replaced.
45. The `vfork()` system call is omitted, and `fork()` is implemented alongside copy-on-write optimizations.
46. A modify bit (dirty bit) is tracked within page table entries to indicate altered pages.
47. The frame-allocation policy uses a proportional allocation algorithm.
48. Page replacement follows an LRU-approximation scheme utilizing the enhanced second-chance algorithm.
49. A pool of free frames is maintained; when a page fault occurs, the desired page is read into an available free frame from the pool before the selected victim frame is written back to disk.
50. A global page-replacement policy is enforced using kernel reaper routines to maintain free memory between maximum and minimum thresholds.
51. Thrashing prevention and the working-set model are not implemented.
52. Kernel memory allocation is handled through the buddy system allocator.


## Chapter 11: Mass-Storage Structure

53. Hard Disk Drive (HDD) requests are sorted using the Circular SCAN (C-SCAN) scheduling algorithm.
54. Non-Volatile Memory (NVM) devices use a First-Come-First-Serve (FCFS) scheduling policy enhanced by merging adjacent write requests.
55. If time constraints emerge, development priority is given to the NVM scheduling implementation over the HDD implementation.
56. Data integrity and error detection are verified using Cyclic Redundancy Check (CRC) codes, omitting error correction.
57. Device mounting, file-system mounting abstractions, and configuration layouts like `/etc/fstab` are skipped.
58. The kernel binary and system files reside on a FAT32 system partition; the Rust `bootloader` crate loads the kernel from this partition during initialization. No custom bootstrap block logic is hardcoded into the disk image.
59. Bad block detection and bad block correction routines are omitted.
60. The swap partition is backed by a dedicated RamDisk (currently 4 MiB / 1024 pages for testing) addressed directly by the kernel's swap manager via page-sized slots. In deployment this would be a fixed 2 GB raw region on the QEMU disk image, but the slot-based abstraction is identical at either size. The FAT32 system partition holds the kernel, user files, and metadata overlay. General filesystem mounting support is not required because the system recognizes exactly these two fixed partition roles.
61. Tertiary storage systems, network-attached storage, and cloud storage are not supported.
62. RAID configurations are not supported.


## Chapter 12: I/O Systems

63. Access to device controllers is handled using memory-mapped I/O.
64. I/O operations are entirely interrupt-driven, bypassing continuous register polling techniques.
65. The interrupt structure is split into a First-Level Interrupt Handler (FLIH) and a Second-Level Interrupt Handler (SLIH).
66. The RamDisk is memory-mapped into kernel virtual space, so I/O transfers are direct `memcpy` operations rather than DMA-based double buffering. A real hardware driver would add a DMA path behind the same `DiskDevice` trait.
67. The standard `ioctl()` system call interface is implemented for device manipulation.
68. Device drivers adhere to standard application I/O interface abstractions.
69. Block devices expose a uniform interface containing `read()`, `write()`, and `seek()` operations.
70. Character-stream devices expose a uniform interface containing `get()` and `put()` operations.
71. Network socket interfaces are excluded from the I/O subsystem.
72. High-resolution kernel timers and related timing subsystems are not implemented.
73. The system exposes both blocking and non-blocking I/O interfaces managed via an internal device-status table.
74. All hardware I/O instructions are designated as privileged instructions restricted to kernel mode execution.
75. UNIX System V STREAMS are not supported.
76. Low-level I/O request rescheduling is not implemented.


## Chapter 13: File-System Interface

77. Case sensitivity for all file and directory names is enforced at the kernel syscall layer. The underlying FAT32 volume stores names via Long File Name (LFN) entries; the kernel rejects any `open()` or `create()` call whose name differs only in case from an existing entry.
78. File metadata tracks names, sizes in bytes, directory flag, access protection rights (`rwx` per owner/group/world), and user/group identification owners. Timestamps are delegated to the underlying `fatfs` crate and not surfaced in the kernel `Stat` structure. Ownership and permission fields are not stored natively by FAT32; they are maintained in the kernel-managed metadata overlay described in Decision 106.
79. Supported file operations implemented via system calls include `create()`, `open()`, `write()`, `read()`, `seek()`, `delete()`, and `rename()`. `truncate()` has a syscall number reserved (`SYS_TRUNCATE = 29`) but is not wired in the dispatcher.
80. Hard links and soft/symbolic links are not supported.
81. The kernel maintains a centralized open-file table to track active files.
82. Files must be explicitly opened via `open()` before executing any operations on them, with the exception of `create()`, `delete()`, and `rename()`.
83. File locking is mandatory and exclusive, preventing multiple processes from operating on the same file concurrently. For files governed by Immutable Shared-Files Semantics (see Decision 104), write operations are rejected at the syscall layer before lock acquisition is even attempted.
84. File descriptors are tracked using a two-level table architecture: a per-process open-file table mapping to a system-wide open-file table.
85. The file system adheres to a `NAME.EXTENSION` formatting rule to denote implicit file types.
86. The kernel minimizes the diversity of structural types, treating all files (including executables) under a uniform system representation.
87. All files are treated as raw streams of bytes, where each individual byte is addressable by its offset from the beginning or end of the file.
88. File data access is governed by sequential `read_next(n)` and `write_next(n)` primitives that operate on a per-file cursor. The `seek()` system call repositions the cursor; it does not introduce arbitrary random-access I/O outside of the sequential model.
89. The directory layout uses a tree-structured hierarchy, using a dedicated bit flag to indicate whether an entry represents a file (0) or a subdirectory (1).
90. Distinct system calls are exposed to handle the creation and deletion of directory nodes.
91. Deleting a non-empty directory node recursively deletes all containing files and subdirectories.
92. Access protection follows the standard user/owner, group, and universe model, enforcing individual read, write, and execute (`rwx`) permissions for each tier. Because FAT32 has no native permission fields, these rights are stored in the kernel metadata overlay and enforced entirely within the syscall dispatch layer before any `fatfs` call is made.
93. Memory-mapping of files into a process's virtual memory space is not supported.


## Chapter 14: File-System Implementation

94. The file system software architecture is layered into a Logical File System, a File-Organization Module, a Basic File System, and an I/O Control layer.
95. The native on-disk file system is FAT32, accessed through the `fatfs` crate, a `#![no_std]`-compatible Rust library that handles FAT12/16/32 allocation table management, directory traversal, and LFN entries. No custom file system layout is written from scratch; `ext4` is not used.
96. Creating a new file allocates an in-memory File Control Block (FCB) populated from the FAT32 directory entry returned by the `fatfs` crate. The FCB holds in-memory state - open count, cursor position, dirty flag - alongside ownership and permission data sourced from the kernel metadata overlay.
97. In-memory directory structures are maintained as simple linear lists rather than hash tables.
98. Secondary storage allocation and free-space management are delegated entirely to the `fatfs` crate, which maintains the FAT32 allocation table internally. The kernel does not implement a separate indexed allocation scheme or a free-space bitmap.
99. The file system bypasses buffer caches and page caches, forcing `read()` and `write()` calls to interact directly with the file system layers.
100. Metadata overlay changes (permissions, ownership records, authentication records) use a status bit to signify that an update is in flux; the bit is cleared upon successful completion. If it remains set after an unexpected termination, a consistency checker runs at boot time to verify and repair the `/.system/` overlay files before the scheduler starts.
101. Advanced journaling and log-based recovery techniques are omitted from the file system implementation.


## Chapter 15: File-System Internals

102. The operating system supports only a single native file system: FAT32 via the `fatfs` crate.
103. The Virtual File System (VFS) abstraction layer is not implemented. The call path runs directly from the syscall dispatcher through the kernel's logical file system layer into the `fatfs` crate driver; no VFS object model (inode objects, dentry objects, superblock objects, file objects) is interposed.
104. File sharing is governed by Immutable Shared-Files Semantics: once the creator of a file marks it as shared, its contents become permanently unalterable and its filename cannot be reused after deletion.
105. Network File System (NFS) capabilities are not implemented.
106. A kernel-managed system domain is maintained at the reserved path `/.system/`. This domain contains two binary files loaded into a protected kernel memory region at boot:
     - `auth.db`: salted password hashes for all registered users, keyed by user ID.
     - `perms.db`: per-path permission records mapping each file and directory path to its `<uid, gid, rwx>` triple.

     All reads and writes targeting `/.system/` are intercepted by the kernel and routed through dedicated internal APIs; they are never reachable via the normal file system call interface available to user processes. System configuration data (device initialization parameters and similar) is also stored within this domain rather than in user-accessible configuration files, in line with the kernel-managed system-crate principle.


## Chapter 16: Security

107. File authorization and resource security policies are governed by the principle of least privilege.
108. Any cryptographic operations required by the system are implemented using established third-party library crates rather than written from scratch.
109. User authentication relies on password verification against salted hashes stored in `/.system/auth.db`, which is loaded into kernel memory at boot and is never exposed through normal file system operations. Passwords are never stored in plaintext; `/etc/passwd` is not used.


## Chapter 17: Protection

110. Resource protection boundaries are enforced through Protection Domains organized via an Access Matrix model.
111. The Access Matrix supports core operations consisting of `access(i, j)`, `copy`, `owner`, and `control`.
112. The Access Matrix is implemented as an access list for objects, with entries structured as a set of `<domain, rights-set>` pairs.
113. The Access Matrix is decoupled behind a clean trait interface to allow alternative back-end implementations to be substituted later.
114. Role-Based Access Control (RBAC) is implemented alongside the access list configuration.
115. Mandatory Access Control (MAC) is not supported.
