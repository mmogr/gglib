//! Atomic PID file I/O operations.
//!
//! Format: Two-line text file
//! ```text
//! <pid>
//! <port>
//! ```

use std::fs;
use std::io;
use std::path::PathBuf;

use gglib_core::paths::pids_dir;

/// PID file content parsed from disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PidFileData {
    pub pid: u32,
    pub port: u16,
}

/// Write PID file atomically using temp file + rename.
///
/// # File naming
/// `<model_id>.pid` (e.g., `42.pid`)
///
/// # Atomicity
/// 1. Write to `<model_id>.pid.tmp`
/// 2. Rename to `<model_id>.pid` (atomic on Unix/macOS)
pub fn write_pidfile(model_id: i64, pid: u32, port: u16) -> io::Result<PathBuf> {
    let dir = pids_dir().map_err(io::Error::other)?;
    fs::create_dir_all(&dir)?;

    let filename = format!("{model_id}.pid");
    let final_path = dir.join(&filename);
    let temp_path = dir.join(format!("{filename}.tmp"));

    // Write to temp file
    let content = format!("{pid}\n{port}\n");
    fs::write(&temp_path, content)?;

    // Atomic rename
    fs::rename(&temp_path, &final_path)?;

    Ok(final_path)
}

/// Read PID file content.
pub fn read_pidfile(model_id: i64) -> io::Result<PidFileData> {
    let dir = pids_dir().map_err(io::Error::other)?;
    let path = dir.join(format!("{model_id}.pid"));
    let content = fs::read_to_string(&path)?;

    parse_pidfile_content(&content)
}

/// Delete PID file (idempotent - no error if missing).
pub fn delete_pidfile(model_id: i64) -> io::Result<()> {
    let dir = pids_dir().map_err(io::Error::other)?;
    let path = dir.join(format!("{model_id}.pid"));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// List all PID files in the directory.
///
/// Returns `(model_id, PidFileData)` pairs for successfully parsed files.
/// Silently ignores malformed files.
pub fn list_pidfiles() -> io::Result<Vec<(i64, PidFileData)>> {
    let dir = pids_dir().map_err(io::Error::other)?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&dir)?;
    let mut results = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("pid") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        let Ok(model_id) = stem.parse::<i64>() else {
            continue;
        };

        if let Ok(content) = fs::read_to_string(&path)
            && let Ok(data) = parse_pidfile_content(&content)
        {
            results.push((model_id, data));
        }
    }

    Ok(results)
}

fn parse_pidfile_content(content: &str) -> io::Result<PidFileData> {
    let mut lines = content.lines();

    let pid = lines
        .next()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing or invalid PID"))?;

    let port = lines
        .next()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing or invalid port"))?;

    Ok(PidFileData { pid, port })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::paths::isolate_data_root;
    use std::fs;

    /// In this binary's own data root (#955), which every test here that
    /// writes a pidfile shares, so the id is one no other test uses.
    #[test]
    fn roundtrip_pidfile() {
        isolate_data_root();
        let model_id = 999_011;
        let pid = 98765;
        let port = 8080;

        let path = write_pidfile(model_id, pid, port).expect("write failed");
        assert!(path.exists());

        let data = read_pidfile(model_id).expect("read failed");
        assert_eq!(data.pid, pid);
        assert_eq!(data.port, port);

        delete_pidfile(model_id).expect("delete failed");
        assert!(!path.exists());

        // Second delete should be idempotent
        delete_pidfile(model_id).expect("second delete failed");
    }

    /// A file named for a model but not `.pid` is not a pidfile, even when
    /// what it holds parses as one. Other tests' pidfiles may sit beside it,
    /// so only this test's id is counted.
    #[test]
    fn list_pidfiles_filters_non_pid_files() {
        isolate_data_root();
        let dir = pids_dir().expect("pids_dir failed");
        let test_id = 99999;
        write_pidfile(test_id, 100, 8080).expect("write failed");
        let decoy = dir.join(format!("{test_id}.txt"));
        fs::write(&decoy, "200\n8080\n").expect("write failed");

        let list = list_pidfiles().expect("list failed");
        delete_pidfile(test_id).expect("cleanup failed");
        fs::remove_file(&decoy).ok();

        let ours: Vec<_> = list.into_iter().filter(|(id, _)| *id == test_id).collect();
        assert_eq!(
            ours,
            [(
                test_id,
                PidFileData {
                    pid: 100,
                    port: 8080
                }
            )]
        );
    }
}
