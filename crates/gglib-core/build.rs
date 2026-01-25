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
    println!("cargo:rerun-if-changed=../../Cargo.toml");

    let readme_path = Path::new(crate_dir).join("README.md");
    let content = if readme_path.exists() {
        fs::read_to_string(readme_path).unwrap()
    } else {
        return; // No README, nothing to process
    };

    // Get repository URL from workspace Cargo.toml for cross-doc links
    let repo_url = get_workspace_repo_url(crate_dir);

    // Transform for rustdoc:
    // 1. Strip 'src/' prefix from links so rustdoc can resolve modules
    // 2. Strip '.rs' extension so links go to modules, not files
    // 3. Convert relative README links to absolute repo URLs (no local rustdoc target exists)
    let mut rustdoc_content = content
        .replace("](src/", "](")
        .replace(".rs)", ")");

    // Transform ../../README.md links to repo URL (agnostic - reads from Cargo.toml)
    if let Some(url) = &repo_url {
        rustdoc_content = rustdoc_content.replace("](../../README.md", &format!("]({}", url));
    }

    // Write to OUT_DIR (cargo's build artifact directory)
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("README_GENERATED.md");
    fs::write(dest_path, rustdoc_content).unwrap();
}

/// Extract repository URL from workspace Cargo.toml
fn get_workspace_repo_url(crate_dir: &str) -> Option<String> {
    let workspace_toml = Path::new(crate_dir)
        .parent()? // crates/
        .parent()? // workspace root
        .join("Cargo.toml");

    let content = fs::read_to_string(workspace_toml).ok()?;

    // Simple extraction: find repository = "..." line
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("repository") && line.contains('=') {
            // Extract URL from: repository = "https://..."
            if let Some(start) = line.find('"') {
                if let Some(end) = line.rfind('"') {
                    if start < end {
                        return Some(line[start + 1..end].to_string());
                    }
                }
            }
        }
    }
    None
}
