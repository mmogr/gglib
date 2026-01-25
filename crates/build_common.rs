// Shared build script utilities for README-to-rustdoc transformation.
// Include this in build.rs files with: include!("../build_common.rs");
//
// Required imports in the including file:
//   use std::env;
//   use std::fs;
//   use std::path::Path;

/// Process a crate's README.md for rustdoc, applying necessary link transformations.
///
/// Transformations:
/// 1. Strip 'src/' prefix from links so rustdoc can resolve modules
/// 2. Strip '.rs' extension so links go to modules, not files  
/// 3. Convert relative README links (../../README.md) to absolute repo URLs
///
/// The repo URL is read from workspace Cargo.toml, keeping READMEs URL-agnostic.
fn process_readme_for_rustdoc(crate_dir: &str) {
    println!("cargo:rerun-if-changed=README.md");
    println!("cargo:rerun-if-changed=../../Cargo.toml");

    let readme_path = Path::new(crate_dir).join("README.md");
    let Ok(content) = fs::read_to_string(&readme_path) else {
        return; // No README, nothing to process
    };

    // Get repository URL from workspace Cargo.toml for cross-doc links
    let repo_url = get_workspace_repo_url(crate_dir);

    // Apply transformations
    let mut rustdoc_content = content
        .replace("](src/", "](")
        .replace(".rs)", ")");

    // Transform ../../README.md links to repo URL (agnostic - reads from Cargo.toml)
    if let Some(url) = &repo_url {
        rustdoc_content = rustdoc_content.replace("](../../README.md", &format!("]({url}"));
    }

    // Write to OUT_DIR
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("README_GENERATED.md");
    fs::write(dest_path, rustdoc_content).unwrap();
}

/// Extract repository URL from workspace Cargo.toml.
/// Returns None if the file can't be read or doesn't contain a repository field.
fn get_workspace_repo_url(crate_dir: &str) -> Option<String> {
    let workspace_toml = Path::new(crate_dir)
        .parent()? // crates/
        .parent()? // workspace root
        .join("Cargo.toml");

    let content = fs::read_to_string(workspace_toml).ok()?;

    // Simple extraction: find repository = "..." line
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("repository")
            && line.contains('=')
            && let Some(start) = line.find('"')
            && let Some(end) = line.rfind('"')
            && start < end
        {
            return Some(line[start + 1..end].to_string());
        }
    }
    None
}
