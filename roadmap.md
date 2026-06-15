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

## Step 5  Heap Activation
- `main.rs`: added `BOOTLOADER_CONFIG` with `physical_memory_offset = FixedAddress(0xFFFF_8000_0000_0000)` and `kernel_stack_size = 64 * 4096`; replaced bare `entry_point!(kernel_main)` with `entry_point!(kernel_main, config = &BOOTLOADER_CONFIG)`; uncommented `extern crate alloc` and `memory::init(boot_info)`
- `arch/mod.rs`: removed unused `BootInfo` parameter from `init()` - it was causing a double-mutable-borrow compile error since the bootloader requires the reference to be `'static`
- Verified with a `Box<u64>` smoke test printing the heap address over serial; deleted after confirmation

## Step 6  Task Struct
- `process/task.rs`: `TaskState` enum (`Running`, `Ready`, `Blocked`, `Zombie`); `Task` struct with `Pid`, `Tid`, `TaskState`, `rsp: u64`, `cr3: u64`, `preempt_count: u32`, `priority: u8`, `parent: Option<Pid>`, `children: Vec<Pid>`, and a private `kernel_stack: Vec<u8>`; constructor allocates a fixed 64 KiB kernel stack and populates an initial stack frame so that `switch_to`'s `ret` lands at `entry`; global `PREEMPT_COUNT` stub with `preempt_disable()`, `preempt_enable()`, `preempt_count()` - to be replaced by per-task field access once `CURRENT` exists in the scheduler
- `sync/spinlock.rs`: added `as_ptr() -> *mut T` method needed by `prepare_switch` to extract raw task pointers without holding the inner lock

## Step 7  Context Switch
- `arch/context.rs`: `switch_to(outgoing: *mut Task, incoming: *const Task)` as a `#[naked]` function; pushes the six callee-saved registers (`rbx`, `rbp`, `r12`–`r15`) onto the outgoing stack, saves `rsp` into `outgoing.rsp`, loads `rsp` from `incoming.rsp`, pops the six registers, and returns - which resumes the incoming task at wherever it last called `switch_to`; uses `offset_of!(Task, rsp)` to locate the field without hard-coding an offset
- `arch/mod.rs`: added `pub mod context`

## Step 8  MLFQ Scheduler
- `scheduler/mlfq.rs`: `MlfqScheduler` with eight priority queues (`VecDeque<Arc<Spinlock<Task>>>`); time slices grow quadratically (`(i+1)²` ticks); `tick()` decrements `ticks_remaining` and sets `needs_reschedule`; `prepare_switch()` demotes the outgoing task by one level (not to zero), re-queues it, promotes the incoming task to `Running`, and returns raw pointers for `switch_to`; `boost_priorities()` moves every queued task up one level (not to queue 0) every 100 ticks to prevent starvation; `pick_next()` scans queues from highest to lowest priority
- `scheduler/mod.rs`: global `SCHEDULER: Lazy<Spinlock<MlfqScheduler>>`; `init()` creates the idle task at lowest priority and installs it as current; `spawn()`, `tick()`, `schedule()` as the public surface; `schedule()` bails immediately if `preempt_count > 0`; idle task spins on `hlt`
- `arch/idt.rs`: timer FLIH now calls `scheduler::tick()` then `scheduler::schedule()` after sending EOI; EOI is sent before `schedule()` so the PIC is not blocked if `switch_to` suspends the handler mid-flight
