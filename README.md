# AlaphiOS

In Georgian, *alaphi* (ალაფი; ნადავლი) means *trophy*, or more precisely, something well deserved (literally, spoils of the hunt).

## What Is AlaphiOS?

AlaphiOS is a hobby x86-64 operating system kernel built as a deliberate exercise in applying the concepts covered in the textbook "Operating System Concepts" (often called the "Dinosaur Book"; see Reference section below) from first principles. It is not optimised for production use or performance benchmarking; architectural decisions were made because the approach is beautiful, challenging, or both.

The kernel targets **QEMU x86-64 emulation exclusively** and is designed for the desktop/laptop computer class of hardware. It provides a terminal-style text interface, a minimal but complete set of system calls for multi-threaded execution, and a layered file system built on FAT32.

## Before reading the source code

Please read [`architectural_decisions.md`](./architectural_decisions.md). It documents every significant design choice made for this project, organised chapter-by-chapter alongside the "Dinosaur Book". Understanding *why* things are built the way they are is essential context for the *how*.

## Scope

The following is a high-level summary of what AlaphiOS implements. All details and the reasoning behind each decision live in [`architectural_decisions.md`](./architectural_decisions.md).

**Kernel & Processes**
- Monolithic `#![no_std]` kernel, loaded via the Rust `bootloader` crate
- Process Control Block (PCB) with unified `task` abstraction (Linux-style, no hard process/thread distinction)
- IPC: message passing (fixed-size, indirect) + shared memory (POSIX-like bounded buffer) + ordinary pipes

**Threads & Scheduling**
- One-to-one user-to-kernel thread mapping with POSIX-compatible `fork()` / `join()` semantics
- Guard pages for stack overflow detection
- Fully preemptive kernel with `preempt_count` per-task tracking
- Single-core Multilevel Feedback Queue (MLFQ) scheduler with Round-Robin at equal priority

**Synchronisation & Deadlocks**
- Monitors (spinlock + condition variable + wait queue), `trylock()` variants exposed via syscall
- x86 `LOCK`-prefix atomic integer module for non-blocking shared-counter operations
- Statically defined global lock hierarchy to eliminate circular-wait

**Memory Management**
- Multi-level hierarchical (forward-mapped) page table; 4 KB pages and frames
- Demand paging with zero-fill-on-demand, copy-on-write `fork()`
- Enhanced second-chance LRU-approximation page replacement
- Proportional frame allocation with global reaper routines (min/max thresholds)
- Buddy system allocator for kernel memory
- TLB flushed on every context switch

**Mass Storage & I/O**
- Virtual disk: FAT32 system partition + dedicated 2 GB raw swap partition
- HDD: C-SCAN scheduling; NVM: FCFS with adjacent-write merging
- CRC error detection (no correction)
- Fully interrupt-driven I/O (FLIH + SLIH split); memory-mapped device controller access
- Double buffering for large DMA transfers; `ioctl()` system call interface

**File System**
- FAT32 via the `fatfs` crate (`#![no_std]`-compatible); no VFS layer
- Case-sensitive naming enforced at the syscall layer over LFN entries
- Mandatory exclusive file locking; two-level file descriptor table
- Kernel-managed `/.system/` domain: `auth.db` (salted password hashes) + `perms.db` (`uid/gid/rwx` overlay)
- Immutable Shared-Files Semantics for shared files
- Boot-time consistency checker for the metadata overlay

**Security & Protection**
- Principle of least privilege; Access Matrix (implemented as per-object access lists) with RBAC
- Password authentication against salted hashes - no plaintext storage, no `/etc/passwd`
- Cryptographic operations delegated to established third-party crates

**Out of Scope**
- Networking (no sockets, no NFS)
- Virtual machines
- GUI / graphical framebuffer output
- SMP / multi-core scheduling
- VFS, dynamic linking, memory-mapped files, RAID, journaling



## Directory Structure

```
alaphios/
├── architectural_decisions.md   # Design decisions (read this first)
├── Cargo.toml                   # Workspace manifest
├── Cargo.lock
├── LICENSE                      # GNU GPL v3.0
├── README.md
├── kernel/                      # Kernel crate (#![no_std])
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Kernel entry point (_start / kernel_main)
│       ├── arch/                # x86-64-specific code
│       │   ├── mod.rs
│       │   ├── gdt.rs           # Global Descriptor Table
│       │   ├── idt.rs           # Interrupt Descriptor Table
│       │   ├── interrupts.rs    # FLIH / SLIH handlers
│       │   └── paging.rs        # Page table management, TLB flush
│       ├── process/             # PCB, task abstraction, fork/join
│       ├── scheduler/           # MLFQ scheduler
│       ├── memory/              # Buddy allocator, frame allocator, demand paging
│       ├── sync/                # Spinlocks, mutexes, condition variables, monitors
│       ├── ipc/                 # Message passing, shared memory, pipes
│       ├── fs/                  # File system layers (logical → fatfs)
│       ├── io/                  # Device drivers, interrupt-driven I/O, ioctl
│       ├── security/            # Access matrix, RBAC, auth.db interface
│       └── syscall/             # Syscall dispatch table
├── bootloader-config/           # bootloader crate configuration
└── tools/                       # Host-side utilities (disk image creation, etc.)
```

> The directory structure above reflects the intended organisation at the start of the project. It will probably evolve as implementation progresses.

## Prerequisites & Setup

### Assumed installed

This project assumes **Rust 1.96.0 / Cargo 1.96.0** (stable) is already present on your system. If your toolchain differs, run:

```bash
rustup install 1.96.0
rustup default 1.96.0
```

You will also need the `x86_64-unknown-none` bare-metal target and the `llvm-tools` component:

```bash
rustup target add x86_64-unknown-none
rustup component add llvm-tools-preview
```

### Installing QEMU

AlaphiOS runs exclusively inside **QEMU x86-64**. Install it for your platform:

**Ubuntu / Debian (including WSL2)**
```bash
sudo apt update
sudo apt install qemu-system-x86
```

**Arch Linux**
```bash
sudo pacman -S qemu-system-x86
```

**Windows (native, outside WSL2)**
Download the installer from the [official QEMU website](https://www.qemu.org/download/#windows) and add the install directory to your `PATH`.

Verify the installation:
```bash
qemu-system-x86_64 --version
```

### Usage

All commands must be run from inside the `kernel/` directory:

```bash
cd kernel
```

Build only (no QEMU):
```bash
cargo build
```

Build + boot in QEMU:
```bash
cargo run
```

Build + boot with GDB stub paused at entry:
```bash
ALAPHIOS_DEBUG=1 cargo run -p kernel
```
then in another terminal:
```bash
gdb -ex 'target remote :1234' target/x86_64-unknown-none/debug/kernel
```

## Reference

Silberschatz, A., Galvin, P. B., & Gagne, G. (2018). *Operating System Concepts* (10th ed.). Wiley.
