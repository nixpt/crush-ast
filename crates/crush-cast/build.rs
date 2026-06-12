use std::path::PathBuf;

fn main() {
    let crate_root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Ensure output directories exist for export binaries.
    let bindings_dir = crate_root.join("bindings");
    if !bindings_dir.exists() {
        std::fs::create_dir_all(&bindings_dir).unwrap();
    }
    let python_dir = crate_root.join("python");
    if !python_dir.exists() {
        std::fs::create_dir_all(&python_dir).unwrap();
    }

    // Re-run build.rs whenever source files change.
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/types.rs");
    println!("cargo:rerun-if-changed=src/ai.rs");
    println!("cargo:rerun-if-changed=src/format.rs");
    println!("cargo:rerun-if-changed=src/bin/export-py.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
