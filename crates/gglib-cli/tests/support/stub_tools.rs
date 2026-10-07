//! A machine made of stand-in commands: a directory to use as `PATH`, holding
//! shell scripts that answer as the real tools would and write down how they
//! were called.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.
//! Nothing here runs a real git, compiler or installer, and nothing a test
//! starts with one of these directories as its whole `PATH` can.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// One test at a time writes its stand-ins and runs them: a script another
/// thread is still writing cannot be executed. A test that makes a
/// [`Machine`] holds this from its first line to its last.
pub(crate) fn one_at_a_time() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A `PATH` of stand-ins, and the file they record their calls in.
pub(crate) struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    /// A machine with nothing on its `PATH`.
    pub(crate) fn bare() -> Self {
        let dir = tempfile::tempdir().expect("a directory for the stand-ins");
        std::fs::create_dir(dir.path().join("bin")).expect("its bin directory");
        Self { dir }
    }

    /// The directory to give a child as its whole `PATH`.
    pub(crate) fn path(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    /// Where the stand-ins record their calls, one `name arguments` a line.
    pub(crate) fn record(&self) -> PathBuf {
        self.dir.path().join("calls")
    }

    /// The calls recorded so far.
    pub(crate) fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.record())
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Add `name`, a shell script that records its call and then runs
    /// `body`, which can append to the record itself, as `$CALLS`.
    pub(crate) fn with(self, name: &str, body: &str) -> Self {
        let script = format!(
            "#!/bin/sh\nCALLS='{}'\nprintf '%s\\n' \"{name} $*\" >> \"$CALLS\"\n{body}\n",
            self.record().display()
        );
        let path = self.path().join(name);
        std::fs::write(&path, script).expect("a stand-in is written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("a stand-in is made executable");
        self
    }

    /// Add `name`, which prints `banner` whatever it is asked.
    pub(crate) fn with_tool(self, name: &str, banner: &str) -> Self {
        self.with(name, &format!("echo '{banner}'"))
    }
}
