//! A library a newer gglib upgraded, met by the binary: the command is
//! handed to a copy on `PATH` that opens it, or refused with that copy
//! named; and what the binary says after it upgrades a library itself.
//!
//! The binary runs with a data root, a home and a daemon port of its own
//! and a `PATH` that holds only directories the test made, so the only
//! copies it asks anything are the stand-ins here.

use gglib_core::utils::process::cmd;
use std::path::{Path, PathBuf};
use std::process::Output;

/// A machine in a temporary directory.
struct Machine {
    root: tempfile::TempDir,
    port: u16,
}

impl Machine {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("a directory for it all");
        std::fs::create_dir(root.path().join("home")).expect("a home");
        // A port nothing answers on, so no daemon of the owner's is reached.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("a free port")
            .port();
        Self { root, port }
    }

    fn database(&self) -> PathBuf {
        self.root.path().join("data").join("data").join("gglib.db")
    }

    /// A library one generation past this build's.
    fn newer_library(&self) {
        let db = self.database();
        std::fs::create_dir_all(db.parent().expect("a parent")).expect("the data folder");
        block_on(gglib_db::write_newer_database(&db)).expect("a newer library");
    }

    /// A stand-in `gglib` of `version` in the folder `name`, which says it
    /// opens `generation`. Whatever it is asked, it prints its version, its
    /// generation and the arguments it was given, and exits 0.
    fn stand_in(&self, name: &str, version: &str, generation: i64) -> PathBuf {
        let folder = self.root.path().join(name);
        std::fs::create_dir(&folder).expect("its folder");
        let args = gglib_test_proc::ARGS;
        gglib_test_proc::install(
            &folder,
            "gglib",
            &format!(
                "print gglib {version} (0123456789ab)\n\
                 print schema_generation = {generation}\n\
                 print stand-in ran: {args}\n"
            ),
        )
    }

    /// A stand-in as [`Self::stand_in`], but a shell script: given a
    /// command, it says whether it was handed over, and exits 7.
    #[cfg(unix)]
    fn script(&self, name: &str, version: &str, generation: i64) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = self.root.path().join(name);
        std::fs::create_dir(&folder).expect("its folder");
        let path = folder.join("gglib");
        let body = format!(
            "#!/bin/sh\n\
             if [ \"$1\" = --version ]; then echo 'gglib {version} (0123456789ab)'; exit 0; fi\n\
             if [ \"$1 $2\" = 'config paths' ]; then echo 'schema_generation = {generation}'; exit 0; fi\n\
             echo \"stand-in ran: $* (handed over: ${{GGLIB_HANDED_OVER:-no}})\"\n\
             exit 7\n"
        );
        std::fs::write(&path, body).expect("a stand-in");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("its mode");
        path
    }

    /// `bin <args>` on this machine, with the folders of `on_path` as its
    /// whole `PATH` and `env` set.
    fn run(&self, bin: &Path, on_path: &[&Path], env: &[(&str, &str)], args: &[&str]) -> Output {
        let home = self.root.path().join("home");
        let dirs = on_path.iter().map(|copy| copy.parent().expect("a folder"));
        cmd(bin)
            .args(args)
            .current_dir(&home)
            .env("GGLIB_DATA_DIR", self.root.path().join("data"))
            .env("GGLIB_DAEMON_PORT", self.port.to_string())
            .env("HOME", &home)
            .env("PATH", std::env::join_paths(dirs).expect("a PATH"))
            .env_remove("GGLIB_MODELS_DIR")
            .env_remove("GGLIB_RESOURCE_DIR")
            .env_remove(gglib_cli::HANDED_OVER)
            .envs(env.iter().copied())
            .output()
            .expect("gglib runs")
    }
}

/// The real binary where cargo built it. With it on `PATH`, its folder is
/// one of the folders there, as an installed copy's is.
fn installed() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gglib"))
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(future)
}

fn out(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn err(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn text(output: &Output) -> String {
    format!("stdout: {}\nstderr: {}", out(output), err(output))
}

/// On Windows nothing is handed over, and the copy to run is named
/// (`a_copy_handed_a_command_and_the_service_hand_nothing_on`).
// Unix only: handing over replaces this process with the other copy (`exec`).
#[cfg(unix)]
#[test]
fn an_installed_copy_hands_its_command_to_one_that_opens_the_library() {
    let machine = Machine::new();
    machine.newer_library();
    let installed = installed();
    let newer = machine.script("newer", "9.9.9", 99);

    let got = machine.run(&installed, &[&installed, &newer], &[], &["config", "show"]);

    assert_eq!(got.status.code(), Some(7), "{}", text(&got));
    assert_eq!(
        out(&got),
        "stand-in ran: config show (handed over: 1)\n",
        "{}",
        text(&got)
    );
    let said = err(&got);
    assert!(said.contains("is too old for your library"), "{said}");
    assert!(
        said.contains(&format!(
            "{} (9.9.9) is running this command",
            newer.display()
        )),
        "{said}"
    );
}

/// Windows hands nothing over: an installed copy beside one that opens the
/// library names that copy, exits with an error, and does not run it.
// Windows only: everywhere else this is the case that is handed over.
#[cfg(windows)]
#[test]
fn on_windows_an_installed_copy_names_the_copy_to_run_and_runs_nothing() {
    let machine = Machine::new();
    machine.newer_library();
    let installed = installed();
    let newer = machine.stand_in("newer", "9.9.9", 99);

    let got = machine.run(&installed, &[&installed, &newer], &[], &["config", "show"]);

    assert_eq!(got.status.code(), Some(1), "{}", text(&got));
    assert!(out(&got).is_empty(), "{}", text(&got));
    let said = err(&got);
    assert!(
        said.contains(&format!(
            "{} (9.9.9) opens it: run that one",
            newer.display()
        )),
        "{said}"
    );
    assert!(!said.contains("is running this command"), "{said}");
}

#[test]
fn a_copy_that_opens_no_newer_library_is_not_handed_the_command() {
    let machine = Machine::new();
    machine.newer_library();
    let installed = installed();
    let as_old = machine.stand_in("as-old", "0.37.0", 99);

    let got = machine.run(&installed, &[&installed, &as_old], &[], &["config", "show"]);

    assert_eq!(got.status.code(), Some(1), "{}", text(&got));
    assert!(out(&got).is_empty(), "{}", text(&got));
    assert!(
        err(&got).contains("No other gglib on PATH opens it"),
        "{}",
        text(&got)
    );
}

#[test]
fn a_build_run_from_its_folder_is_refused_with_the_copy_to_run() {
    let machine = Machine::new();
    machine.newer_library();
    let newer = machine.stand_in("newer", "9.9.9", 99);

    let built = installed();
    let got = machine.run(&built, &[&newer], &[], &["config", "show"]);

    assert_eq!(got.status.code(), Some(1), "{}", text(&got));
    assert!(out(&got).is_empty(), "{}", text(&got));
    let said = err(&got);
    assert!(
        said.contains(&format!("{} (9.9.9) opens it", newer.display())),
        "{said}"
    );
    assert!(said.contains("GGLIB_DATA_DIR"), "{said}");
}

#[test]
fn a_copy_handed_a_command_and_the_service_hand_nothing_on() {
    let machine = Machine::new();
    machine.newer_library();
    let installed = installed();
    let newer = machine.stand_in("newer", "9.9.9", 99);
    let on_path = [installed.as_path(), newer.as_path()];

    let handed = [(gglib_cli::HANDED_OVER, "1")];
    for (env, args) in [
        (&handed[..], &["config", "show"][..]),
        (&[][..], &["daemon", "run"][..]),
    ] {
        let got = machine.run(&installed, &on_path, env, args);

        assert_eq!(got.status.code(), Some(1), "gglib {args:?}\n{}", text(&got));
        assert!(out(&got).is_empty(), "gglib {args:?}\n{}", text(&got));
        assert!(
            err(&got).contains("opens it: run that one"),
            "gglib {args:?}\n{}",
            text(&got)
        );
    }
}

#[test]
fn an_upgrade_names_the_copy_it_leaves_behind_once() {
    let machine = Machine::new();
    let installed = installed();
    let older = machine.stand_in("older", "0.37.0", 0);
    let on_path = [installed.as_path(), older.as_path()];
    let made = machine.run(&installed, &on_path, &[], &["config", "show"]);
    assert!(made.status.success(), "{}", text(&made));
    assert!(
        !err(&made).contains("no longer opens it"),
        "a new library is no upgrade\n{}",
        text(&made)
    );
    block_on(gglib_db::restamp_older(&machine.database())).expect("an older library");

    let upgraded = machine.run(&installed, &on_path, &[], &["config", "show"]);

    assert!(upgraded.status.success(), "{}", text(&upgraded));
    let said = err(&upgraded);
    assert!(said.contains("no longer opens it"), "{said}");
    assert!(
        said.contains(&format!("{} (0.37.0)", older.display())),
        "{said}"
    );
    let again = machine.run(&installed, &on_path, &[], &["config", "show"]);
    assert!(
        !err(&again).contains("no longer opens it"),
        "{}",
        text(&again)
    );
}
