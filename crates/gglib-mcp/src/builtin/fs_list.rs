//! `builtin:list_directory` tool implementation.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::sandboxing::resolve_sandboxed_path;

/// List entries of a directory within the sandbox.
///
/// Returns one entry per line: directories end with `/`, files are plain.
/// Hidden files (starting with `.`) are excluded by default unless
/// `include_hidden` is true.
pub fn list_directory(
    args: &HashMap<String, Value>,
    sandbox_root: &Path,
) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(".");

    let resolved = resolve_sandboxed_path(sandbox_root, path)?;

    if !resolved.is_dir() {
        return Err(format!("'{}' is not a directory", path));
    }

    let include_hidden = args
        .get("include_hidden")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut entries: Vec<String> = Vec::new();

    let read_dir = std::fs::read_dir(&resolved)
        .map_err(|e| format!("failed to read directory '{}': {e}", path))?;

    for entry in read_dir {
        let entry = entry.map_err(|e| format!("directory entry error: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();

        if !include_hidden && name.starts_with('.') {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|e| format!("cannot determine type of '{}': {e}", name))?;

        if file_type.is_dir() {
            entries.push(format!("{name}/"));
        } else {
            entries.push(name);
        }
    }

    entries.sort();

    if entries.is_empty() {
        Ok("(empty directory)".to_string())
    } else {
        Ok(entries.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn args_with(path: &str) -> HashMap<String, Value> {
        let mut m = HashMap::new();
        m.insert("path".to_string(), Value::String(path.to_string()));
        m
    }

    #[test]
    fn lists_files_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "hi").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let result = list_directory(&args_with("."), dir.path()).unwrap();
        assert!(result.contains("file.txt"));
        assert!(result.contains("subdir/"));
    }

    #[test]
    fn hides_dotfiles_by_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".hidden"), "secret").unwrap();
        fs::write(dir.path().join("visible"), "hi").unwrap();

        let result = list_directory(&args_with("."), dir.path()).unwrap();
        assert!(!result.contains(".hidden"));
        assert!(result.contains("visible"));
    }

    #[test]
    fn shows_dotfiles_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".hidden"), "secret").unwrap();

        let mut args = args_with(".");
        args.insert("include_hidden".to_string(), Value::Bool(true));

        let result = list_directory(&args, dir.path()).unwrap();
        assert!(result.contains(".hidden"));
    }

    #[test]
    fn not_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "hi").unwrap();

        let err = list_directory(&args_with("file.txt"), dir.path()).unwrap_err();
        assert!(err.contains("not a directory"));
    }

    #[test]
    fn defaults_to_current_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "hi").unwrap();

        // No "path" arg → defaults to "."
        let result = list_directory(&HashMap::new(), dir.path()).unwrap();
        assert!(result.contains("file.txt"));
    }

    #[test]
    fn empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("empty");
        fs::create_dir(&sub).unwrap();

        let result = list_directory(&args_with("empty"), dir.path()).unwrap();
        assert_eq!(result, "(empty directory)");
    }
}
