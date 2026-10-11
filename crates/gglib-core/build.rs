use std::env;
use std::fs;
use std::path::{Path, PathBuf};

include!("../build_common.rs");

fn main() {
    // Get the repo root directory at build time.
    // CARGO_MANIFEST_DIR for gglib-core is crates/gglib-core, so we go up two levels.
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let crate_path = PathBuf::from(&manifest_dir);

    // Navigate to workspace root (two directories up from crates/gglib-core)
    let repo_root = crate_path
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .map_or_else(|| crate_path.clone(), std::path::Path::to_path_buf);

    // Emit this as a compile-time environment variable
    println!(
        "cargo:rustc-env=GGLIB_REPO_ROOT={}",
        repo_root.to_string_lossy()
    );

    // Process README for rustdoc (uses shared build_common.rs)
    process_readme_for_rustdoc(&manifest_dir);

    println!("cargo:rerun-if-changed=build.rs");
}
