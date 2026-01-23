use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Copy linker script to OUT_DIR
    let link_x = include_str!("link.x");
    fs::write(out_dir.join("link.x"), link_x).unwrap();

    // Tell cargo to look for libraries in OUT_DIR
    println!("cargo:rustc-link-search={}", out_dir.display());

    // Tell the linker to use our linker script
    println!("cargo:rustc-link-arg=-Tlink.x");

    // Rerun if linker script changes
    println!("cargo:rerun-if-changed=link.x");
    println!("cargo:rerun-if-changed=build.rs");
}
