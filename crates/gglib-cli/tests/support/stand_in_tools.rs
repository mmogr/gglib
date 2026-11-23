//! A machine made of stand-in commands, the same on every OS: a directory to
//! use as `PATH`, holding programs (`gglib-test-proc`) that answer as the
//! real tools would and write down how they were called.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.
//! Nothing here runs a real git, compiler or installer, and nothing a test
//! starts with one of these directories as its whole `PATH` can.
//!
//! A stand-in answers by its arguments through `gglib-test-proc`'s `when`
//! lines; with none, it does the same whatever it is asked.

use std::path::PathBuf;

/// A `PATH` of stand-ins, and the files they record their calls in.
pub(crate) struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    /// A machine with nothing on its `PATH`.
    pub(crate) fn bare() -> Self {
        let dir = tempfile::tempdir().expect("a directory for the stand-ins");
        for part in ["bin", "calls"] {
            std::fs::create_dir(dir.path().join(part)).expect("its directory");
        }
        Self { dir }
    }

    /// The directory to give a child as its whole `PATH`.
    pub(crate) fn path(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    /// Every call recorded so far, as `name arguments`: each tool's in the
    /// order they were made, and the tools in the order of their names.
    pub(crate) fn calls(&self) -> Vec<String> {
        let mut records: Vec<PathBuf> = std::fs::read_dir(self.dir.path().join("calls"))
            .expect("the records")
            .map(|entry| entry.expect("a record").path())
            .collect();
        records.sort();
        records
            .iter()
            .flat_map(|record| {
                let name = record.file_name().expect("a name").to_string_lossy();
                std::fs::read_to_string(record)
                    .expect("a record is read")
                    .lines()
                    .map(|arguments| format!("{name} {arguments}"))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Add `name`, a program that records its call and then follows
    /// `instructions` (`gglib-test-proc`'s).
    pub(crate) fn with(self, name: &str, instructions: &str) -> Self {
        let record = self.dir.path().join("calls").join(name);
        gglib_test_proc::install(
            &self.path(),
            name,
            &format!("calls {}\n{instructions}", record.display()),
        );
        self
    }

    /// Add `name`, which prints `banner`, line by line, whatever it is
    /// asked.
    pub(crate) fn with_tool(self, name: &str, banner: &str) -> Self {
        let mut prints = String::new();
        for line in banner.lines() {
            prints.push_str("print ");
            prints.push_str(line);
            prints.push('\n');
        }
        self.with(name, &prints)
    }
}
