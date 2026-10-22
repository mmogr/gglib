//! `builtin:grep_search` tool implementation.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use super::sandboxing::{Resolved, is_binary, resolve_sandboxed_path};

/// Search for a pattern in files within the sandbox.
///
/// Does a case-insensitive substring search across all text files under
/// the given path (default: sandbox root).  Returns matching lines with
/// file paths and line numbers, relative to the root.
///
/// A walk stops at the next entry once `stop` is set: a call whose wait the
/// tool timeout gave up on sets it, so the walk does not go on unread.
pub(crate) fn grep_search(
    args: &HashMap<String, Value>,
    sandbox_root: &Path,
    stop: &AtomicBool,
) -> Result<String, String> {
    const MAX_MATCHES: usize = 200;

    let pattern = args
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or("missing required argument 'pattern'")?;

    if pattern.is_empty() {
        return Err("'pattern' must not be empty".to_string());
    }

    let search_path = args.get("path").and_then(Value::as_str).unwrap_or(".");

    let Resolved { path, shown, root } = resolve_sandboxed_path(sandbox_root, search_path)?;
    let walk = Walk {
        root,
        pattern: pattern.to_lowercase(),
        max: MAX_MATCHES,
        stop,
    };
    let mut matches = Vec::new();

    if path.is_file() {
        walk.search_file(&path, &mut matches);
    } else if path.is_dir() {
        walk.walk_dir(&path, &mut vec![path.clone()], &mut matches);
    } else {
        return Err(format!("'{shown}' is not a file or directory"));
    }

    if matches.is_empty() {
        Ok(format!("no matches found for '{pattern}'"))
    } else {
        let truncated = matches.len() >= MAX_MATCHES;
        let mut result = matches.join("\n");
        if truncated {
            use std::fmt::Write;
            let _ = write!(result, "\n\n[results truncated at {MAX_MATCHES} matches]");
        }
        Ok(result)
    }
}

/// One search: what it looks for, under which canonical root, and when it
/// stops.
struct Walk<'a> {
    /// The canonical sandbox root. Nothing outside it is read.
    root: PathBuf,
    /// The pattern, lowercased.
    pattern: String,
    /// The most matches collected.
    max: usize,
    /// Set when nobody waits for the result any more.
    stop: &'a AtomicBool,
}

impl Walk<'_> {
    /// Whether the walk is over: enough matches, or nobody waiting.
    fn done(&self, matches: &[String]) -> bool {
        matches.len() >= self.max || self.stop.load(Ordering::Relaxed)
    }

    /// Search a single file for matching lines.
    fn search_file(&self, path: &Path, matches: &mut Vec<String>) {
        if is_binary(path) || matches.len() >= self.max {
            return;
        }

        let Ok(content) = fs::read_to_string(path) else {
            return;
        };

        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string();

        for (line_num, line) in content.lines().enumerate() {
            if matches.len() >= self.max {
                break;
            }
            if line.to_lowercase().contains(&self.pattern) {
                matches.push(format!("{}:{}:{}", relative, line_num + 1, line));
            }
        }
    }

    /// Recursively walk a directory searching files.
    ///
    /// `ancestors` holds the canonical form of each directory on the way down
    /// to `dir`, `dir`'s own last. An entry's own type decides what it is. A
    /// symlink is followed only when its canonical target lies under the
    /// root, so a link to a folder or file outside it is never read. A
    /// directory already on the way down is not walked again, so a link to
    /// `.` or to an ancestor lists each file once.
    ///
    /// The target is checked once and then read through the link by name, so
    /// a link swapped in between is read where it then points. None of the
    /// builtins writes, so only another process can swap one.
    fn walk_dir(&self, dir: &Path, ancestors: &mut Vec<PathBuf>, matches: &mut Vec<String>) {
        // Skip common noise directories
        const SKIP_DIRS: &[&str] = &[
            "node_modules",
            ".git",
            "target",
            "__pycache__",
            ".venv",
            "dist",
            "build",
            "web_ui",
        ];

        let Some(here) = ancestors.last().cloned() else {
            return;
        };
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };

        let mut sorted: Vec<_> = entries.filter_map(std::result::Result::ok).collect();
        sorted.sort_by_key(std::fs::DirEntry::file_name);

        for entry in sorted {
            if self.done(matches) {
                return;
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with('.') && name != "." {
                continue;
            }

            let Ok(mut file_type) = entry.file_type() else {
                continue;
            };
            let target = if file_type.is_symlink() {
                let Ok(canon) = path.canonicalize() else {
                    continue;
                };
                if !canon.starts_with(&self.root) {
                    continue;
                }
                let Ok(meta) = fs::metadata(&canon) else {
                    continue;
                };
                file_type = meta.file_type();
                Some(canon)
            } else {
                None
            };

            if file_type.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                // A plain entry under a canonical directory is canonical by
                // its name; a link's is its target.
                let canon = target.unwrap_or_else(|| here.join(entry.file_name()));
                if ancestors.contains(&canon) {
                    continue;
                }
                ancestors.push(canon);
                self.walk_dir(&path, ancestors, matches);
                ancestors.pop();
            } else if file_type.is_file() {
                self.search_file(&path, matches);
            }
        }
    }
}

#[cfg(test)]
#[path = "fs_grep_tests.rs"]
mod tests;
