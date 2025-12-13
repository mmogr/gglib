//! Build script for gglib-lib crate.
//!
//! Sets `GGLIB_REPO_ROOT` environment variable for runtime path resolution.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Get the repo root directory at build time
    let repo_root = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Emit this as a compile-time environment variable
    println!("cargo:rustc-env=GGLIB_REPO_ROOT={repo_root}");

    // Also write it to a file for reference
    let data_dir = PathBuf::from(&repo_root).join("data");
    fs::create_dir_all(&data_dir).unwrap();

    let config_file = data_dir.join(".gglib_repo_path");
    fs::write(&config_file, repo_root).unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
