//! `builtin:grep_search` tool implementation.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::sandboxing::{is_binary, resolve_sandboxed_path};

/// Search for a pattern in files within the sandbox.
///
/// Does a case-insensitive substring search across all text files under
/// the given path (default: sandbox root).  Returns matching lines with
/// file paths and line numbers.
pub fn grep_search(
    args: &HashMap<String, Value>,
    sandbox_root: &Path,
) -> Result<String, String> {
    let pattern = args
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or("missing required argument 'pattern'")?;

    if pattern.is_empty() {
        return Err("'pattern' must not be empty".to_string());
    }

    let search_path = args
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(".");

    let resolved = resolve_sandboxed_path(sandbox_root, search_path)?;

    let pattern_lower = pattern.to_lowercase();
    let mut matches = Vec::new();

    const MAX_MATCHES: usize = 200;

    if resolved.is_file() {
        search_file(&resolved, sandbox_root, &pattern_lower, &mut matches, MAX_MATCHES);
    } else if resolved.is_dir() {
        walk_dir(&resolved, sandbox_root, &pattern_lower, &mut matches, MAX_MATCHES);
    } else {
        return Err(format!("'{}' is not a file or directory", search_path));
    }

    if matches.is_empty() {
        Ok(format!("no matches found for '{}'", pattern))
    } else {
        let truncated = matches.len() >= MAX_MATCHES;
        let mut result = matches.join("\n");
        if truncated {
            result.push_str(&format!("\n\n[results truncated at {} matches]", MAX_MATCHES));
        }
        Ok(result)
    }
}

/// Search a single file for matching lines.
fn search_file(
    path: &Path,
    sandbox_root: &Path,
    pattern: &str,
    matches: &mut Vec<String>,
    max: usize,
) {
    if is_binary(path) || matches.len() >= max {
        return;
    }

    let Ok(content) = fs::read_to_string(path) else {
        return;
    };

    let relative = path
        .strip_prefix(sandbox_root)
        .unwrap_or(path)
        .display()
        .to_string();

    for (line_num, line) in content.lines().enumerate() {
        if matches.len() >= max {
            break;
        }
        if line.to_lowercase().contains(pattern) {
            matches.push(format!("{}:{}:{}", relative, line_num + 1, line));
        }
    }
}

/// Recursively walk a directory searching files.
fn walk_dir(
    dir: &Path,
    sandbox_root: &Path,
    pattern: &str,
    matches: &mut Vec<String>,
    max: usize,
) {
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

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        if matches.len() >= max {
            return;
        }

        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') && name != "." {
            continue;
        }

        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk_dir(&path, sandbox_root, pattern, matches, max);
            }
        } else if path.is_file() {
            search_file(&path, sandbox_root, pattern, matches, max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with_pattern(pattern: &str) -> HashMap<String, Value> {
        let mut m = HashMap::new();
        m.insert("pattern".to_string(), Value::String(pattern.to_string()));
        m
    }

    fn args_with_pattern_and_path(pattern: &str, path: &str) -> HashMap<String, Value> {
        let mut m = args_with_pattern(pattern);
        m.insert("path".to_string(), Value::String(path.to_string()));
        m
    }

    #[test]
    fn finds_matching_lines() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "fn hello() {}\nfn world() {}\n").unwrap();

        let result = grep_search(&args_with_pattern("hello"), dir.path()).unwrap();
        assert!(result.contains("code.rs:1:fn hello()"));
    }

    #[test]
    fn case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "fn Hello() {}\n").unwrap();

        let result = grep_search(&args_with_pattern("hello"), dir.path()).unwrap();
        assert!(result.contains("Hello"));
    }

    #[test]
    fn no_matches() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "fn hello() {}\n").unwrap();

        let result = grep_search(&args_with_pattern("xyz_no_match"), dir.path()).unwrap();
        assert!(result.contains("no matches"));
    }

    #[test]
    fn searches_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.rs"), "fn deep_func() {}\n").unwrap();

        let result = grep_search(&args_with_pattern("deep_func"), dir.path()).unwrap();
        assert!(result.contains("sub/nested.rs:1:fn deep_func()"));
    }

    #[test]
    fn skips_binary_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("bin.dat"), b"match\x00here").unwrap();

        let result = grep_search(&args_with_pattern("match"), dir.path()).unwrap();
        assert!(result.contains("no matches"));
    }

    #[test]
    fn search_single_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "fn hello() {}\n").unwrap();
        fs::write(dir.path().join("b.rs"), "fn hello() {}\n").unwrap();

        let result =
            grep_search(&args_with_pattern_and_path("hello", "a.rs"), dir.path()).unwrap();
        assert!(result.contains("a.rs"));
        // Should not contain b.rs since we targeted a.rs only
        assert!(!result.contains("b.rs"));
    }

    #[test]
    fn missing_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let err = grep_search(&HashMap::new(), dir.path()).unwrap_err();
        assert!(err.contains("pattern"));
    }

    #[test]
    fn empty_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let err = grep_search(&args_with_pattern(""), dir.path()).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn skips_git_and_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        fs::create_dir(&git).unwrap();
        fs::write(git.join("config"), "match_me\n").unwrap();

        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        fs::write(nm.join("pkg.js"), "match_me\n").unwrap();

        fs::write(dir.path().join("src.rs"), "match_me\n").unwrap();

        let result = grep_search(&args_with_pattern("match_me"), dir.path()).unwrap();
        // Only src.rs should match
        assert!(result.contains("src.rs"));
        assert!(!result.contains(".git"));
        assert!(!result.contains("node_modules"));
    }
}
