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

use gglib_core::paths::{canonical, inside, join};

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
    canonical(sandbox_root).map_err(|e| format!("The working folder cannot be read: {e}"))
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

    // A path with a root that lies under the root is taken as is, and is
    // shown by its part under the root. On Windows a root with no drive
    // (`\proj\notes`) is on the folder's own drive. Any other path with a
    // root has its root components stripped and is rebased inside
    // sandbox_root, so that `/etc/passwd`, `C:\secret` and a driveless
    // `\notes` are treated as relative paths within the sandbox.
    let anchored = if raw.is_absolute() {
        Some(raw.to_path_buf())
    } else if raw.has_root() {
        Some(sandbox_root.join(raw))
    } else {
        None
    };
    let under = anchored
        .as_deref()
        .and_then(|full| under_root(full, sandbox_root, &canon_root));
    let (candidate, shown) = match (under, anchored) {
        (Some(rest), Some(full)) => {
            let shown = if rest.as_os_str().is_empty() {
                ".".to_string()
            } else {
                shown_inside(&rest)
            };
            (full, shown)
        }
        _ if raw.has_root() => {
            use std::path::Component;
            let relative: PathBuf = raw
                .components()
                .filter(|c| !matches!(c, Component::Prefix(_) | Component::RootDir))
                .collect();
            (join(sandbox_root, &relative), user_path.to_string())
        }
        // Joined through the owner, so a name such as `notes.` opens that
        // file and not `notes` under a plain root.
        _ => (join(sandbox_root, raw), user_path.to_string()),
    };

    // Canonicalize to resolve symlinks and `..`
    let path = canonical(&candidate).map_err(|e| match e.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => {
            format!("No file or folder '{shown}' {MISSING}")
        }
        _ => format!("'{shown}' cannot be read: {e}"),
    })?;

    // The root and the path may each be plain or verbatim on Windows.
    if inside(&path, &canon_root).is_none() {
        return Err(format!("'{shown}' {OUTSIDE}."));
    }

    Ok(Resolved {
        path,
        shown,
        root: canon_root,
    })
}

/// A path inside the folder as the tools show it: its parts joined with `/`
/// on every OS, so what a model reads from one tool it can pass to another.
pub(crate) fn shown_inside(rest: &Path) -> String {
    rest.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
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
    if let Some(rest) = inside(raw, canon_root) {
        return Some(rest);
    }
    let canon = raw.ancestors().find_map(|known| {
        let tail = raw.strip_prefix(known).ok()?;
        Some(join(&canonical(known).ok()?, tail))
    })?;
    inside(&canon, canon_root)
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
