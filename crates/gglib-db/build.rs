use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=README.md");
    println!("cargo:rerun-if-changed=../../Cargo.toml");

    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let readme_path = Path::new(&crate_dir).join("README.md");

    let content = if readme_path.exists() {
        fs::read_to_string(readme_path).unwrap()
    } else {
        return;
    };

    // Get repository URL from workspace Cargo.toml for cross-doc links
    let repo_url = get_workspace_repo_url(&crate_dir);

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
