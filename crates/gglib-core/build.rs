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

    // Process README for rustdoc (uses shared build_common.rs)
    process_readme_for_rustdoc(&manifest_dir);

    // Build fingerprint: the git commit this binary was built from, with a
    // -dirty suffix for an unclean tree. Two binaries reporting the same
    // CARGO_PKG_VERSION can still be different code — measured live: a CLI
    // carrying new daemon routes silently used an installed daemon of the
    // same version and got an opaque 405. The fingerprint is what lets the
    // probe say "different build" instead. Falls back to "unknown" outside a
    // git checkout (a release tarball), where version comparison is the best
    // available and skew is far less likely than in a dev tree.
    let fingerprint = git_fingerprint(&repo_root).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=GGLIB_BUILD_FINGERPRINT={fingerprint}");
    // HEAD moves on commit/checkout; index/packed-refs cover branch updates.
    println!(
        "cargo:rerun-if-changed={}/.git/HEAD",
        repo_root.to_string_lossy()
    );

    println!("cargo:rerun-if-changed=build.rs");
}

fn git_fingerprint(repo_root: &Path) -> Option<String> {
    let head = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !head.status.success() {
        return None;
    }
    let hash = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    if hash.is_empty() {
        return None;
    }
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(repo_root)
        .output()
        .ok()
        .is_some_and(|out| out.status.success() && !out.stdout.is_empty());
    Some(if dirty { format!("{hash}-dirty") } else { hash })
}
