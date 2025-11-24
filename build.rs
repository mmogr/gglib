use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "scripts/build_docs.rs"]
mod build_docs;

fn main() {
    // Process README for documentation (rewrite relative links)
    build_docs::process_readme().expect("Failed to process README for docs");
    println!("cargo:rerun-if-changed=README.md");

    // Get the repo root directory at build time
    let repo_root = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Emit this as a compile-time environment variable
    println!("cargo:rustc-env=GGLIB_REPO_ROOT={}", repo_root);

    // Also write it to a file for reference
    let data_dir = PathBuf::from(&repo_root).join("data");
    fs::create_dir_all(&data_dir).unwrap();

    let config_file = data_dir.join(".gglib_repo_path");
    fs::write(&config_file, repo_root).unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
