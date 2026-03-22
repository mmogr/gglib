//! Sandbox path resolution and validation.
//!
//! All filesystem tools must resolve paths through [`resolve_sandboxed_path`]
//! which ensures the resolved path is within the sandbox root directory.
//! Symlinks are resolved before checking containment so they cannot escape
//! the sandbox.

use std::path::{Path, PathBuf};

/// Resolve `user_path` relative to `sandbox_root`, rejecting escapes.
///
/// - Absolute paths are rebased relative to `sandbox_root`.
/// - `..` components that would escape are caught after canonicalization.
/// - Symlinks are resolved (via `canonicalize`) so they cannot point outside.
///
/// Returns the canonical path on success. The `Err` variant is a
/// human-readable message intended for the LLM (not `anyhow::Error`).
pub fn resolve_sandboxed_path(sandbox_root: &Path, user_path: &str) -> Result<PathBuf, String> {
    let user_path = user_path.trim();
    if user_path.is_empty() {
        return Err("path must not be empty".to_string());
    }

    let raw = Path::new(user_path);

    // Build candidate: if absolute, strip the leading `/` and join to sandbox root
    let candidate = if raw.is_absolute() {
        let relative = raw.strip_prefix("/").unwrap_or(raw);
        sandbox_root.join(relative)
    } else {
        sandbox_root.join(raw)
    };

    // Canonicalize both to resolve symlinks and `..`
    let canon_root = sandbox_root
        .canonicalize()
        .map_err(|e| format!("sandbox root does not exist: {e}"))?;

    let canon_candidate = candidate
        .canonicalize()
        .map_err(|e| format!("path '{}' does not exist: {e}", candidate.display()))?;

    if !canon_candidate.starts_with(&canon_root) {
        return Err(format!("path '{user_path}' is outside the sandbox"));
    }

    Ok(canon_candidate)
}

/// Check whether a file appears to be binary by scanning for null bytes.
///
/// Reads at most the first 8 KiB.
pub fn is_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 8192];
    let Ok(n) = f.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn relative_path_stays_inside() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("hello.txt");
        fs::write(&child, "hi").unwrap();

        let resolved = resolve_sandboxed_path(dir.path(), "hello.txt").unwrap();
        assert_eq!(resolved, child.canonicalize().unwrap());
    }

    #[test]
    fn absolute_path_is_rebased() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("hello.txt");
        fs::write(&child, "hi").unwrap();

        let resolved = resolve_sandboxed_path(dir.path(), "/hello.txt").unwrap();
        assert_eq!(resolved, child.canonicalize().unwrap());
    }

    #[test]
    fn dotdot_escape_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("sub");
        fs::create_dir(&child).unwrap();

        let result = resolve_sandboxed_path(&child, "../");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside the sandbox"));
    }

    #[test]
    fn nonexistent_path_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_sandboxed_path(dir.path(), "no_such_file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn empty_path_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_sandboxed_path(dir.path(), "");
        assert!(result.is_err());
    }

    #[test]
    fn is_binary_detects_null_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("binary.dat");
        fs::write(&bin, b"hello\x00world").unwrap();
        assert!(is_binary(&bin));
    }

    #[test]
    fn is_binary_allows_text() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("text.txt");
        fs::write(&txt, "hello world").unwrap();
        assert!(!is_binary(&txt));
    }
}
