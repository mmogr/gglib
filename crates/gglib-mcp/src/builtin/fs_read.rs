//! `builtin:read_file` tool implementation.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::sandboxing::{is_binary, resolve_sandboxed_path};

/// Read a file within the sandbox, returning its content.
///
/// Returns a human-readable error string on failure (not anyhow) so the
/// agent loop receives a graceful tool-error message.
pub fn read_file(args: &HashMap<String, Value>, sandbox_root: &Path) -> Result<String, String> {
    const MAX_CHARS: usize = 100_000;

    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or("missing required argument 'path'")?;

    let resolved = resolve_sandboxed_path(sandbox_root, path)?;

    if is_binary(&resolved) {
        return Err(format!("file '{path}' appears to be binary — skipped"));
    }

    let content =
        fs::read_to_string(&resolved).map_err(|e| format!("failed to read '{path}': {e}"))?;

    if content.len() > MAX_CHARS {
        Ok(format!(
            "{}\n\n[truncated — file exceeds {} characters]",
            &content[..MAX_CHARS],
            MAX_CHARS
        ))
    } else {
        Ok(content)
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
    fn missing_path_arg() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_file(&HashMap::new(), dir.path()).unwrap_err();
        assert!(err.contains("path"));
    }

    #[test]
    fn nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_file(&args_with("nope.txt"), dir.path()).unwrap_err();
        assert!(err.contains("does not exist"));
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
}
