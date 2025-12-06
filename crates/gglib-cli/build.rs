use std::env;

fn main() {
    // Get the workspace root directory at build time
    // CARGO_MANIFEST_DIR is the gglib-cli crate directory
    // We navigate up to the workspace root
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .expect("Could not determine workspace root")
        .to_string_lossy()
        .to_string();

    // Emit this as a compile-time environment variable
    println!("cargo:rustc-env=GGLIB_REPO_ROOT={}", workspace_root);

    println!("cargo:rerun-if-changed=build.rs");
}
