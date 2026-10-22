//! Sandbox path resolution and validation.
//!
//! All filesystem tools must resolve paths through [`resolve_sandboxed_path`]
//! which ensures the resolved path is within the sandbox root directory.
//! Symlinks are resolved before checking containment so they cannot escape
//! the sandbox.
//!
//! The model is told about the root as "the working folder", never as a
//! sandbox, and a path in a refusal is the one it can use again: what it
//! wrote, or the part under the root of an absolute path inside it.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// How a refusal for a path that names nothing ends, after the path.
///
/// The whole message is `No file or folder '<path>' ` followed by this.
pub(crate) const MISSING: &str = "in the working folder. Call list_directory to see what is there.";

/// How a refusal for a path that leaves the root ends, after the path.
///
/// The whole message is `'<path>' ` followed by this and a full stop.
pub(crate) const OUTSIDE: &str = "is outside the working folder";

/// A path the model named, resolved under the root.
#[derive(Debug)]
pub(crate) struct Resolved {
    /// Its canonical form, under `root`.
    pub(crate) path: PathBuf,
    /// How a message names it to the model: what it wrote, or the part under
    /// the root of an absolute path inside it.
    pub(crate) shown: String,
    /// The canonical root.
    pub(crate) root: PathBuf,
}

/// The canonical form of `sandbox_root`, or the refusal the model reads when
/// the folder has gone.
fn canonical_root(sandbox_root: &Path) -> Result<PathBuf, String> {
    sandbox_root
        .canonicalize()
        .map_err(|e| format!("The working folder cannot be read: {e}"))
}

/// Resolve `user_path` relative to `sandbox_root`, rejecting escapes.
///
/// - An absolute path under the root, as given or canonical, is taken as is.
/// - Any other absolute path is rebased relative to `sandbox_root`.
/// - `..` components that would escape are caught after canonicalization.
/// - Symlinks are resolved (via `canonicalize`) so they cannot point outside.
///
/// Returns the canonical path with its shown form on success. The `Err`
/// variant is a human-readable message intended for the LLM (not
/// `anyhow::Error`): a path that names nothing is `MISSING`, one that cannot
/// be read for another reason says why.
pub(crate) fn resolve_sandboxed_path(
    sandbox_root: &Path,
    user_path: &str,
) -> Result<Resolved, String> {
    let user_path = user_path.trim();
    if user_path.is_empty() {
        return Err("path must not be empty".to_string());
    }

    let canon_root = canonical_root(sandbox_root)?;
    let raw = Path::new(user_path);

    // An absolute path under the root is taken as is, and is shown by its
    // part under the root. Any other absolute path has its root components
    // stripped and is rebased inside sandbox_root, so that `/etc/passwd` or
    // `C:\secret` are treated as relative paths within the sandbox.
    let under = if raw.is_absolute() {
        under_root(raw, sandbox_root, &canon_root)
    } else {
        None
    };
    let (candidate, shown) = match under {
        Some(rest) => {
            let shown = if rest.as_os_str().is_empty() {
                ".".to_string()
            } else {
                rest.display().to_string()
            };
            (raw.to_path_buf(), shown)
        }
        None if raw.is_absolute() => {
            use std::path::Component;
            let relative: PathBuf = raw
                .components()
                .filter(|c| !matches!(c, Component::Prefix(_) | Component::RootDir))
                .collect();
            (sandbox_root.join(relative), user_path.to_string())
        }
        None => (sandbox_root.join(raw), user_path.to_string()),
    };

    // Canonicalize to resolve symlinks and `..`
    let path = candidate.canonicalize().map_err(|e| match e.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => {
            format!("No file or folder '{shown}' {MISSING}")
        }
        _ => format!("'{shown}' cannot be read: {e}"),
    })?;

    if !path.starts_with(&canon_root) {
        return Err(format!("'{shown}' {OUTSIDE}."));
    }

    Ok(Resolved {
        path,
        shown,
        root: canon_root,
    })
}

/// The part of the absolute `raw` under the root, when it lies under it:
/// lexically under the root as given or its canonical form, or under the
/// canonical root once its longest existing part is canonicalised, so a miss
/// through another name for the root is still under it. `None` when it lies
/// elsewhere.
fn under_root(raw: &Path, sandbox_root: &Path, canon_root: &Path) -> Option<PathBuf> {
    if let Ok(rest) = raw.strip_prefix(sandbox_root) {
        return Some(rest.to_path_buf());
    }
    if let Ok(rest) = raw.strip_prefix(canon_root) {
        return Some(rest.to_path_buf());
    }
    let canon = raw.ancestors().find_map(|known| {
        let tail = raw.strip_prefix(known).ok()?;
        Some(known.canonicalize().ok()?.join(tail))
    })?;
    canon.strip_prefix(canon_root).ok().map(Path::to_path_buf)
}

/// Check whether a file appears to be binary by scanning for null bytes.
///
/// Reads at most the first 8 KiB.
pub(crate) fn is_binary(path: &Path) -> bool {
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
#[path = "sandboxing_tests.rs"]
mod tests;
