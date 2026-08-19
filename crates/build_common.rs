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
/// 1. Point module-table source links (`](src/cors.rs)`) at the repository
/// 2. Convert relative README links (../../README.md) to absolute repo URLs
///
/// The repo URL is read from workspace Cargo.toml, keeping READMEs URL-agnostic.
///
/// # Why source links and not module links
///
/// This used to strip `src/` and `.rs` so `](src/cors.rs)` became `](cors)` —
/// an intra-doc link to the module. That only resolves for modules that are
/// public, non-ambiguous and actually compiled, and the module tables list
/// every file: private modules, `#[cfg(test)]` modules, and names shared by a
/// function and a module. rustdoc warned on every one of them.
///
/// A row's link text is a *filename*, so a file is the honest target. Pointing
/// at the source on the repo works for every row, needs no visibility
/// analysis, and reads the same on GitHub and in rustdoc.
fn process_readme_for_rustdoc(crate_dir: &str) {
    println!("cargo:rerun-if-changed=README.md");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    // This file is `include!`d into every crate's build.rs, so editing it
    // changes what they all emit — but nothing told cargo that, and a stale
    // build-script binary goes on applying the previous transformation with no
    // sign that it has. Measuring a change to this file is what turned that
    // up, the measurement having been taken against crates that had not
    // rebuilt.
    println!("cargo:rerun-if-changed=../build_common.rs");

    let readme_path = Path::new(crate_dir).join("README.md");
    let Ok(content) = fs::read_to_string(&readme_path) else {
        return; // No README, nothing to process
    };

    // Get repository URL from workspace Cargo.toml for cross-doc links
    let repo_url = get_workspace_repo_url(crate_dir);

    // Apply transformations
    let mut rustdoc_content = content;

    if let Some(url) = &repo_url {
        // Transform ../../README.md links to repo URL (agnostic - reads from Cargo.toml)
        rustdoc_content = rustdoc_content.replace("](../../README.md", &format!("]({url}"));

        // Module-table rows link to source files. Absolutize them so rustdoc
        // renders a working link instead of trying to resolve a module path.
        // Without a repo URL they stay relative, which rustdoc leaves alone.
        if let Some(crate_path) = crate_relative_path(crate_dir) {
            rustdoc_content = absolutize_source_links(&rustdoc_content, url, &crate_path);
        }
    }

    // Write to OUT_DIR
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("README_GENERATED.md");
    fs::write(dest_path, rustdoc_content).unwrap();
}

/// Rewrite every `](src/…)` link to an absolute repository URL.
///
/// Directory rows (`](src/domain/)`) get `tree`, file rows (`](src/cors.rs)`)
/// get `blob`. GitHub redirects `blob` to `tree` for a directory, so the
/// distinction is not load-bearing — it just means the emitted URL is the one
/// that answers rather than the one that bounces.
fn absolutize_source_links(content: &str, repo_url: &str, crate_path: &str) -> String {
    const OPEN: &str = "](src/";

    let mut out = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find(OPEN) {
        let (before, from_open) = rest.split_at(start);
        out.push_str(before);

        let inner_start = OPEN.len();
        let Some(close) = from_open[inner_start..].find(')') else {
            // No closing paren: not a link after all, so copy it through
            // unchanged rather than swallowing the rest of the file.
            out.push_str(from_open);
            return out;
        };
        let path = &from_open[inner_start..inner_start + close];

        let kind = if path.ends_with('/') { "tree" } else { "blob" };
        // Pushed piecewise rather than through `format!`: clippy's
        // `format_push_string` objects to the extra allocation.
        out.push_str("](");
        out.push_str(repo_url);
        out.push('/');
        out.push_str(kind);
        out.push_str("/main/");
        out.push_str(crate_path);
        out.push_str("/src/");
        out.push_str(path);
        out.push(')');

        rest = &from_open[inner_start + close + 1..];
    }

    out.push_str(rest);
    out
}

/// The crate's directory relative to the workspace root, slash-separated for
/// use in a URL (e.g. `crates/gglib-core`).
///
/// Uses the same "two levels up is the workspace root" assumption as
/// [`get_workspace_repo_url`], so the two agree by construction.
fn crate_relative_path(crate_dir: &str) -> Option<String> {
    let path = Path::new(crate_dir);
    let workspace_root = path.parent()?.parent()?;
    let relative = path.strip_prefix(workspace_root).ok()?;
    Some(
        relative
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/"),
    )
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
