use std::path::PathBuf;

fn main() {
    let kernel = PathBuf::from("../target/x86_64-unknown-none/debug/nova_kernel");
    println!("cargo:rerun-if-changed={}", kernel.display());

    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&PathBuf::from("nova_os.img"))
        .unwrap();
}