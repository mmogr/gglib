//! `builtin:read_file` tool implementation.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::sandboxing::{is_binary, resolve_sandboxed_path};

/// The most characters of a file the model is given; a longer file is cut
/// after this many characters, never inside one.
const MAX_CHARS: usize = 100_000;

/// Read a file within the sandbox, returning its content.
///
/// Returns a human-readable error string on failure (not anyhow) so the
/// agent loop receives a graceful tool-error message.
pub(crate) fn read_file(
    args: &HashMap<String, Value>,
    sandbox_root: &Path,
) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or("missing required argument 'path'")?;

    let resolved = resolve_sandboxed_path(sandbox_root, path)?;
    let shown = &resolved.shown;

    if is_binary(&resolved.path) {
        return Err(format!("file '{shown}' appears to be binary — skipped"));
    }

    let content =
        fs::read_to_string(&resolved.path).map_err(|e| format!("failed to read '{shown}': {e}"))?;

    match content.char_indices().nth(MAX_CHARS) {
        Some((end, _)) => Ok(format!(
            "{}\n\n[truncated — file exceeds {MAX_CHARS} characters]",
            &content[..end],
        )),
        None => Ok(content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn args_with(path: &str) -> HashMap<String, Value> {
        let mut m = HashMap::new();
        m.insert("path".to_string(), Value::String(path.to_string()));
        m
    }

    #[test]
    fn reads_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        fs::write(&file, "hello world").unwrap();

        let result = read_file(&args_with("hello.txt"), dir.path()).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn rejects_binary() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data.bin");
        fs::write(&file, b"foo\x00bar").unwrap();

        let err = read_file(&args_with("data.bin"), dir.path()).unwrap_err();
        assert!(err.contains("binary"));
    }

    #[test]
    fn a_refusal_names_an_absolute_path_by_its_part_under_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let file = root.join("data.bin");
        fs::write(&file, b"foo\x00bar").unwrap();

        let err = read_file(&args_with(file.to_str().unwrap()), &root).unwrap_err();
        assert_eq!(err, "file 'data.bin' appears to be binary — skipped");
        assert!(!err.contains(&root.display().to_string()), "{err}");
    }

    #[test]
    fn missing_path_arg() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_file(&HashMap::new(), dir.path()).unwrap_err();
        assert!(err.contains("path"));
    }

    #[test]
    fn nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_file(&args_with("nope.txt"), dir.path()).unwrap_err();
        assert!(err.starts_with("No file or folder 'nope.txt' in the working folder."));
    }

    #[test]
    fn truncates_large_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.txt");
        let content = "x".repeat(200_000);
        fs::write(&file, &content).unwrap();

        let result = read_file(&args_with("big.txt"), dir.path()).unwrap();
        assert!(result.len() < 200_000);
        assert!(result.contains("[truncated"));
    }

    #[test]
    fn truncates_multibyte_content_between_characters() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("accents.txt");
        // Byte MAX_CHARS falls inside the second byte of an `é`.
        let content = format!("a{}", "é".repeat(MAX_CHARS));
        fs::write(&file, &content).unwrap();

        let result = read_file(&args_with("accents.txt"), dir.path()).unwrap();
        let (kept, note) = result.split_once("\n\n").unwrap();
        assert_eq!(kept.chars().count(), MAX_CHARS);
        assert!(kept.starts_with("aé"));
        assert!(note.starts_with("[truncated"));
    }

    #[test]
    fn keeps_a_file_of_exactly_max_chars_whole() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("full.txt");
        let content = "é".repeat(MAX_CHARS);
        fs::write(&file, &content).unwrap();

        let result = read_file(&args_with("full.txt"), dir.path()).unwrap();
        assert_eq!(result, content);
    }
}
