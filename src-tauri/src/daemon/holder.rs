//! The process id of the service this app talks to.
//!
//! The one place gglib keeps it is the record the service's lock writes
//! beside it (`daemon.json` in the data folder, `gglib_axum::DaemonLock`);
//! `/health` does not say it. When the app hosts the service itself, the id
//! is the app's own.
//!
//! The record is only read. Whether its service still runs is not asked of
//! the lock here, because `DaemonLock::is_held` takes the lock for an
//! instant when it is free, and a service starting in that instant would
//! be refused. The command asks the watcher's last poll instead
//! (`allow_service_foreground`). What that leaves:
//!
//! - a service killed since the last poll (a couple of seconds ago at most)
//!   leaves its record behind, and its id is read until the poll notices;
//! - a service starting takes its lock before it writes its record, so for
//!   an instant a held lock sits beside a dead service's old id. It answers
//!   the poll only after its record is written, so the record of a service
//!   the poll found answering is its own.

use std::path::Path;

use gglib_axum::DaemonLock;

/// The process id of the running service on the port this app talks to
/// (`gglib_core::daemon_port`), when there is one.
pub(crate) fn service_pid() -> Option<u32> {
    let dir = gglib_core::paths::data_root().ok()?;
    pid_on(&dir, gglib_core::daemon_port())
}

/// The process id the lock record in `dir` names, when that service
/// listens on `port`. The lock itself is not touched.
fn pid_on(dir: &Path, port: u16) -> Option<u32> {
    DaemonLock::read_holder(dir)
        .filter(|holder| holder.port == port)
        .map(|holder| holder.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record a running service keeps names it: this test process,
    /// which holds the lock, on the port it took it for.
    #[test]
    fn a_running_service_is_named_by_its_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _lock = DaemonLock::acquire(dir.path(), 9887).expect("the lock");
        assert_eq!(pid_on(dir.path(), 9887), Some(std::process::id()));
    }

    /// A service on another port is not the one this app talks to.
    #[test]
    fn a_service_on_another_port_is_not_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _lock = DaemonLock::acquire(dir.path(), 9887).expect("the lock");
        assert_eq!(pid_on(dir.path(), 9888), None);
    }

    /// No record, and the record of a service that stopped (it removes its
    /// record as it goes), name nobody. A record a killed service left
    /// behind is read as it is: whether a service answers is the watcher's
    /// poll to say, and the command asks it first.
    #[test]
    fn no_record_is_no_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(pid_on(dir.path(), 9887), None);

        let lock = DaemonLock::acquire(dir.path(), 9887).expect("the lock");
        drop(lock);
        assert_eq!(pid_on(dir.path(), 9887), None);

        std::fs::write(
            dir.path().join("daemon.json"),
            r#"{"pid":4242,"port":9887}"#,
        )
        .expect("a record left behind");
        assert_eq!(pid_on(dir.path(), 9887), Some(4242));
    }
}
