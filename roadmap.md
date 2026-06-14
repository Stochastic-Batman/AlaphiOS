# What was done (and in what order)

## Step 0  Build Configuration
Workspace `Cargo.toml` with `kernel` as the only member and `tools/image-builder` excluded. Kernel configured as a `#![no_std]` binary targeting `x86_64-unknown-none` with `build-std` for `core`, `compiler_builtins`, and `alloc`. Image-builder wired as a host-side tool via `run.sh` invoked by cargo's runner. Several tooling issues resolved: `main.rs` in wrong directory, missing `builder` feature, `build-std` leaking into host builds fixed by `cd`ing to workspace root in `run.sh`.

## Step 1  Leaf Modules (no intra-kernel dependencies)
- `sync/atomic.rs`: `KernelAtomicUsize` newtype over `core::sync::atomic`, emitting x86 `LOCK`-prefixed instructions through the standard atomic API
- `sync/spinlock.rs`: `Spinlock<T>` with `lock()`, `try_lock()`, RAII guard with `Deref`/`DerefMut`/`Drop`, preempt count stub via global `KernelAtomicUsize`
- `process/pid.rs`: `Pid` and `Tid` newtypes over `u64` with atomic allocators starting at 1; PID 0 reserved for idle

## Step 2  Boot Entry + Serial Output
`main.rs` with `bootloader_api::entry_point!`, `uart_16550::SerialPort` at COM1 (`0x3F8`) wrapped in `spin::Mutex`, `serial_print!`/`serial_println!` macros delegating through `_print()` so `core::fmt::Write` is imported in one place, and a panic handler that prints via serial then halts.

## Step 3  Architecture Layer
- `arch/gdt.rs`: GDT with null, kernel code/data (ring 0), user data/code (ring 3), TSS descriptor; TSS with 8 KiB double-fault stack at IST[0]
- `arch/idt.rs`: IDT with breakpoint, double fault (IST[0]), page fault, GPF exception handlers; timer (IRQ0 -> 0x20) and keyboard (IRQ1 -> 0x21) FLIH handlers
- `arch/interrupts.rs`: 8259A PIC initialisation remapping IRQs to 0x20–0x2F; global tick counter; 256-slot scancode ring buffer using head/tail atomics; `tick()`, `push_scancode()`, `pop_scancode()`, `end_of_interrupt()`
- `arch/paging.rs`: `PageMapper` wrapping `OffsetPageTable` with `map_page`, `unmap_page`, `translate_addr`, `flush_tlb`, `flush_tlb_all`

## Step 4  Physical Memory
- `memory/frame_allocator.rs`: `FrameAlloc` trait (the single swap point for allocator implementations); `BootFrameAllocator` scanning bootloader memory regions sequentially, `deallocate` is a no-op until Phase 5
- `memory/buddy_allocator.rs`: power-of-two free lists with intrusive linked list nodes (next pointer stored in the free block itself); split-on-alloc and buddy-coalesce-on-free; wrapped in `LockedBuddyAllocator` implementing `GlobalAlloc`
- `memory/heap.rs`: maps 8 MiB at `0xFFFF_C000_0000_0000` via `PageMapper`, hands the range to the buddy allocator; `#[global_allocator]` registration so `Box`/`Vec`/`Arc` will work once `extern crate alloc` is uncommented
