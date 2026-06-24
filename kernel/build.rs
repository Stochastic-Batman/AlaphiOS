use std::path::Path;
use std::process::Command;

fn main() {
    let init_dir = Path::new("../user/init");
    let out_dir = std::env::var("OUT_DIR").unwrap();

    println!("cargo::rerun-if-changed={}", init_dir.join("init.S").display());
    println!("cargo::rerun-if-changed={}", init_dir.join("init.ld").display());

    let obj = format!("{out_dir}/init.o");
    let elf = format!("{out_dir}/init.elf");
    let bin = format!("{out_dir}/init.bin");

    run(Command::new("as")
        .args(["--64", "-o", &obj])
        .arg(init_dir.join("init.S")));

    run(Command::new("ld")
        .args(["-T"])
        .arg(init_dir.join("init.ld"))
        .args(["-o", &elf, &obj]));

    run(Command::new("objcopy")
        .args(["-O", "binary", &elf, &bin]));
}

fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    assert!(status.success(), "{cmd:?} failed with {status}");
}
