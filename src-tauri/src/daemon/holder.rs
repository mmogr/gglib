//! The process id of the service this app talks to.

#[cfg(test)]
mod tests {
    use super::*;

    use gglib_axum::DaemonLock;

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

    /// No record, a record whose service has stopped, and a record left by
    /// a service that was killed (its lock gone with it) name nobody: the
    /// id may be another program's by now.
    #[test]
    fn no_running_service_is_no_one() {
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
        assert_eq!(pid_on(dir.path(), 9887), None);
    }
}
