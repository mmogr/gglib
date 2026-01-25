use std::{
    env, fs,
    path::{Path, PathBuf},
};

use vergen_gix::{Emitter, GixBuilder};

fn main() {
    // Always rerun when this build script changes.
    println!("cargo:rerun-if-changed=build.rs");

    // Process README for rustdoc
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&manifest_dir);

    // Allow CI or packagers to provide a SHA without any git probing.
    println!("cargo:rerun-if-env-changed=GGLIB_BUILD_SHA_SHORT");

    if let Some(override_sha) = env::var("GGLIB_BUILD_SHA_SHORT")
        .ok()
        .and_then(|s| normalize_sha_short(&s))
    {
        emit_vergen_fallbacks(Some(&override_sha));
        return;
    }

    // Best-effort git probing via vergen-gix, but NEVER fail the build.
    // If no repo is found, we emit explicit fallbacks so `env!()` never fails.
    let Some(repo_root) = find_repo_root(Path::new(
        &env::var("CARGO_MANIFEST_DIR").unwrap_or_default(),
    )) else {
        emit_vergen_fallbacks(None);
        return;
    };

    let git = match GixBuilder::default()
        .repo_path(Some(repo_root))
        .sha(true) // short SHA
        .dirty(false)
        .build()
    {
        Ok(git) => git,
        Err(err) => {
            println!("cargo:warning=gglib-build-info: vergen-gix config failed: {err}");
            emit_vergen_fallbacks(None);
            return;
        }
    };

    if let Err(err) = Emitter::default()
        .add_instructions(&git)
        .and_then(|e| e.emit())
    {
        println!("cargo:warning=gglib-build-info: vergen-gix emit failed: {err}");
        emit_vergen_fallbacks(None);
    }
}

fn emit_vergen_fallbacks(sha_short: Option<&str>) {
    // These are the env vars the crate uses via `env!()`.
    // They MUST always be set, or compilation will fail.
    let sha = sha_short.unwrap_or("unknown");
    println!("cargo:rustc-env=VERGEN_GIT_SHA={sha}");
    println!("cargo:rustc-env=VERGEN_GIT_DIRTY=false");
}

fn normalize_sha_short(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.len() >= 7 {
        &trimmed[..7]
    } else {
        trimmed
    };

    if candidate.len() == 7 && candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
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
