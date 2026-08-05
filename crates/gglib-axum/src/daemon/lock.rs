//! The singleton lock: one daemon per machine, enforced by the OS.
//!
//! An exclusive advisory file lock on `<dir>/daemon.lock`. The lock is held
//! for the owning process's lifetime and released by the kernel on any exit
//! — clean, crashed, or SIGKILLed — so there is no stale state to recover
//! from. The file's *contents* (`{"pid":…,"port":…}`) are advisory metadata
//! for the refusal message and for `gglib daemon status`; the lock itself is
//! what enforces exclusivity.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// What the lock file records about its holder.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LockInfo {
    /// Process id of the running daemon.
    pub pid: u32,
    /// Management-API port the daemon is bound to.
    pub port: u16,
}

/// Failure to acquire the daemon lock.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Another daemon holds the lock. `holder` is what it wrote into the
    /// lock file — `None` when the file could not be parsed (e.g. the holder
    /// died mid-write; the lock is still authoritative).
    #[error("daemon already running{}", holder_suffix(.holder))]
    AlreadyRunning {
        /// The running daemon's recorded pid/port, when readable.
        holder: Option<LockInfo>,
    },
    /// The lock file could not be created, locked, or written.
    #[error("could not acquire daemon lock: {0}")]
    Io(#[from] std::io::Error),
}

fn holder_suffix(holder: &Option<LockInfo>) -> String {
    match holder {
        Some(info) => format!(" (pid {}) at http://127.0.0.1:{}", info.pid, info.port),
        None => String::new(),
    }
}

/// An acquired singleton lock. Held for the daemon's lifetime; the OS
/// releases the underlying lock on process exit no matter how the process
/// dies.
#[derive(Debug)]
pub struct DaemonLock {
    file: File,
    path: PathBuf,
}

impl DaemonLock {
    /// Take the exclusive daemon lock in `dir`, recording this process's pid
    /// and the daemon `port` for whoever finds the lock held.
    ///
    /// # Errors
    ///
    /// [`LockError::AlreadyRunning`] when another process holds the lock
    /// (with its recorded pid/port when readable), [`LockError::Io`] for
    /// filesystem failures.
    pub fn acquire(dir: &Path, port: u16) -> Result<Self, LockError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("daemon.lock");

        // Read+write, no truncate: on conflict the current holder's metadata
        // must still be readable.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                let mut contents = String::new();
                let _ = file.read_to_string(&mut contents);
                let holder = serde_json::from_str::<LockInfo>(contents.trim()).ok();
                return Err(LockError::AlreadyRunning { holder });
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(LockError::Io(e)),
        }

        // Lock is ours — replace whatever a previous (dead) holder left.
        let info = LockInfo {
            pid: std::process::id(),
            port,
        };
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(serde_json::to_string(&info)?.as_bytes())?;
        file.flush()?;

        Ok(Self { file, path })
    }

    /// Read a lock file's recorded holder without taking the lock.
    ///
    /// For `gglib daemon status`: says who *claims* to hold the lock. `None`
    /// when the file is absent or unparseable. The health probe, not this,
    /// is the authority on whether a daemon is actually alive.
    pub fn read_holder(dir: &Path) -> Option<LockInfo> {
        let contents = std::fs::read_to_string(dir.join("daemon.lock")).ok()?;
        serde_json::from_str(contents.trim()).ok()
    }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        // Best-effort tidiness only — the OS releases the lock regardless,
        // and a leftover file cannot block the next acquire.
        let _ = self.file.unlock();
        let _ = std::fs::remove_file(&self.path);
    }
}

// serde_json::Error → io::Error for the `?` in acquire.
impl From<serde_json::Error> for LockError {
    fn from(e: serde_json::Error) -> Self {
        Self::Io(std::io::Error::other(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The core singleton guarantee: a second acquire in the same directory
    /// fails while the first lock is held, and reports the holder.
    #[test]
    fn second_acquire_fails_while_first_is_held() {
        let dir = tempfile::tempdir().unwrap();

        let first = DaemonLock::acquire(dir.path(), 9887).expect("first acquire must succeed");

        let err = DaemonLock::acquire(dir.path(), 9887)
            .expect_err("second acquire must fail while the first lock is held");
        match err {
            LockError::AlreadyRunning { holder } => {
                let holder = holder.expect("holder metadata should be readable");
                assert_eq!(holder.pid, std::process::id());
                assert_eq!(holder.port, 9887);
            }
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }

        drop(first);
    }

    /// Dropping the lock releases it: the next acquire succeeds.
    #[test]
    fn acquire_succeeds_after_release() {
        let dir = tempfile::tempdir().unwrap();

        let first = DaemonLock::acquire(dir.path(), 9887).unwrap();
        drop(first);

        DaemonLock::acquire(dir.path(), 9887)
            .expect("acquire must succeed once the previous lock is dropped");
    }

    /// A leftover lock file from a dead process (file present, lock not
    /// held) must not block acquisition — the OS lock, not the file's
    /// existence, is the authority.
    #[test]
    fn a_stale_lock_file_does_not_block_acquisition() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("daemon.lock"),
            r#"{"pid":4294967295,"port":9887}"#,
        )
        .unwrap();

        DaemonLock::acquire(dir.path(), 9887)
            .expect("an unlocked leftover file must not block acquisition");
    }

    /// The refusal message names the holder so the user knows what to do.
    #[test]
    fn refusal_message_names_pid_and_address() {
        let dir = tempfile::tempdir().unwrap();
        let _held = DaemonLock::acquire(dir.path(), 9887).unwrap();

        let msg = DaemonLock::acquire(dir.path(), 9887)
            .unwrap_err()
            .to_string();
        assert!(msg.contains(&std::process::id().to_string()), "{msg}");
        assert!(msg.contains("http://127.0.0.1:9887"), "{msg}");
    }

    #[test]
    fn read_holder_reports_the_recorded_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let _held = DaemonLock::acquire(dir.path(), 1234).unwrap();

        let holder = DaemonLock::read_holder(dir.path()).expect("holder should be readable");
        assert_eq!(holder.port, 1234);
        assert_eq!(holder.pid, std::process::id());
    }

    #[test]
    fn read_holder_is_none_when_no_lock_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        assert!(DaemonLock::read_holder(dir.path()).is_none());
    }
}
