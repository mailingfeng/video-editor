fn main() {
    // Build-specific encoding only raises string-search cost; this is not a
    // cryptographic secret and all decoded filters remain observable in memory.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    println!("cargo:rustc-env=FRAMESHIFT_TEMPLATE_SEED={seed}");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "desktop")]
    tauri_build::build();
}
