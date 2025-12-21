use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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

    // Create the marker file so release builds can detect they're running from repo
    let data_dir = repo_root.join("data");
    if let Err(e) = fs::create_dir_all(&data_dir) {
        eprintln!("Warning: Failed to create data directory: {e}");
    } else {
        let marker_file = data_dir.join(".gglib_repo_path");
        if let Err(e) = fs::write(&marker_file, repo_root.to_string_lossy().as_bytes()) {
            eprintln!("Warning: Failed to write repo marker file: {e}");
        }
    }

    // Process README for rustdoc
    process_readme_for_rustdoc(&manifest_dir);

    println!("cargo:rerun-if-changed=build.rs");
}

fn process_readme_for_rustdoc(crate_dir: &str) {
    println!("cargo:rerun-if-changed=README.md");

    let readme_path = Path::new(crate_dir).join("README.md");
    let content = if readme_path.exists() {
        fs::read_to_string(readme_path).unwrap()
    } else {
        return; // No README, nothing to process
    };

    // Transform for rustdoc:
    // 1. Strip 'src/' prefix from links so rustdoc can resolve modules
    // 2. Strip '.rs' extension so links go to modules, not files
    let rustdoc_content = content.replace("](src/", "](").replace(".rs)", ")");

    // Write to OUT_DIR (cargo's build artifact directory)
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("README_GENERATED.md");
    fs::write(dest_path, rustdoc_content).unwrap();
}
