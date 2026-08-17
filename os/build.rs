use std::path::PathBuf;

fn main() {
    let kernel = PathBuf::from("../target/x86_64-unknown-none/debug/nova_kernel");

    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&PathBuf::from("nova_os.img"))
        .unwrap();
}