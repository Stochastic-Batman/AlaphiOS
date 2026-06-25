// kernel should be loaded via bootloader crate.
// GDT must be set up before any interrupt can be handled. 
use crate::arch::syscall::PerCpuScratch;
use x86_64::registers::model_specific::KernelGsBase;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
use spin::Lazy;


pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;  // Index into the IST for the double-fault handler stack (0-based).
static BOOT_SCRATCH: PerCpuScratch = PerCpuScratch::new(0);  // kernel_rsp=0 is safe at boot: no user task exists yet so syscall_entry won't fire


pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
    pub tss: SegmentSelector,
}


// Must be 'static because the CPU holds a pointer to it forever.
static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 8192;
        static STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = VirtAddr::from_ptr(&STACK);
        stack_start + STACK_SIZE as u64
    };
    tss
});

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let kernel_code = gdt.append(Descriptor::kernel_code_segment());
    let kernel_data = gdt.append(Descriptor::kernel_data_segment());
    let user_data = gdt.append(Descriptor::user_data_segment());
    let user_code = gdt.append(Descriptor::user_code_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (gdt, Selectors { kernel_code, kernel_data, user_code, user_data, tss })
});


pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, SS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.kernel_code);
        SS::set_reg(GDT.1.kernel_data);
        load_tss(GDT.1.tss);
    }

    KernelGsBase::write(VirtAddr::new(&BOOT_SCRATCH as *const PerCpuScratch as u64));
}


pub fn syscall_selectors() -> (SegmentSelector, SegmentSelector) {
    (GDT.1.kernel_code, GDT.1.user_code)
}


pub fn user_selectors() -> (u16, u16) {
    (GDT.1.user_code.0 | 3, GDT.1.user_data.0 | 3)
}


pub fn set_kernel_stack(stack_top: VirtAddr) {
    let tss = &*TSS as *const TaskStateSegment as *mut TaskStateSegment;
    unsafe { (*tss).privilege_stack_table[0] = stack_top; }
}
