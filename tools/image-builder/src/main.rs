use bootloader::DiskImageBuilder;
use std::{path::PathBuf, process::Command};


fn main() {
    let kernel_binary: PathBuf = std::env::args()
        .nth(1)
        .expect("usage: image-builder <kernel-binary>")
        .into();

    assert!(
        kernel_binary.exists(),
        "kernel binary not found: {}",
        kernel_binary.display()
    );

    // Place the disk image alongside the kernel binary (e.g. target/.../kernel.img).
    let disk_image = kernel_binary.with_extension("img");

    DiskImageBuilder::new(kernel_binary)
        .create_bios_image(&disk_image)
        .expect("failed to create BIOS disk image");

    println!("disk image: {}", disk_image.display());

    let mut qemu = Command::new("qemu-system-x86_64");
    qemu.args([
        "-drive",
        &format!("format=raw,file={}", disk_image.display()),
        "-m", "256M",
        // Serial port -> stdout: this is the primary debug output channel.
        // On WSL2 without WSLg the QEMU window may not be reachable, but
        // uart_16550 output always appears here.
        "-serial", "stdio",
        // Stop on triple fault rather than silently rebooting.
        "-no-reboot",
    ]);

    if std::env::var_os("ALAPHIOS_DEBUG").is_some() {
        // -s  shorthand for -gdb tcp::1234
        // -S  pause CPU at startup; attach before code runs
        qemu.args(["-s", "-S"]);
        eprintln!("GDB server on port 1234 — attach with:");
        eprintln!("  gdb -ex 'target remote :1234' target/x86_64-unknown-none/debug/kernel");
    }

    std::process::exit(
        qemu.status()
            .expect("failed to spawn qemu-system-x86_64")
            .code()
            .unwrap_or(1),
    );
}
