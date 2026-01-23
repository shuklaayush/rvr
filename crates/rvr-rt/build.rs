fn main() {
    // Note: Linker script (link.x) and linker flags must be set in the
    // final binary's .cargo/config.toml, not here. Cargo's rustc-link-arg
    // from library dependencies doesn't propagate to final binary linking.
    //
    // Example .cargo/config.toml for RISC-V projects using rvr-rt:
    //
    // [target.rv64i]
    // rustflags = ["-C", "link-arg=-T<path>/link.x", "-C", "link-arg=--gc-sections"]

    println!("cargo:rerun-if-changed=build.rs");
}
