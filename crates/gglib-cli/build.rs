use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Process README for rustdoc (uses shared build_common.rs)
    process_readme_for_rustdoc(&crate_dir);

    // Keep existing workspace root logic
    let workspace_root = Path::new(&crate_dir)
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .expect("Could not determine workspace root")
        .to_string_lossy()
        .to_string();

    println!("cargo:rustc-env=GGLIB_REPO_ROOT={}", workspace_root);
}
