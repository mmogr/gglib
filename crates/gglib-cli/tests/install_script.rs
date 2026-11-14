//! `packaging/install.sh`, run on a machine made of stand-ins, for what it
//! says about the gglib already there before it installs anything.
//!
//! The script runs as `sh` with an emptied environment and a `PATH` that
//! holds only directories the test made. `id` answers 0, so the script runs
//! its package manager directly; `sudo`, `curl`, `tee`, `apt-get`, `pacman`
//! and `dnf` are scripts that write down their call and do nothing else.
//! Every `gglib` and desktop app it finds is a stand-in too. The only real
//! programs are the shell and, in the tests that say so, links to this
//! machine's `timeout`, `sleep` and `realpath`. Nothing here can reach the
//! real `/etc`, a real package manager or a real gglib.

#![cfg(unix)]

#[path = "support/stub_tools.rs"]
mod stub_tools;

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use stub_tools::{Machine, one_at_a_time};

/// Where the packages put gglib, as the script names it.
const PACKAGED: &str = "packaged=/usr/bin\n";

/// A `timeout` that drops `-s KILL 2` and runs the rest, so a test that
/// needs no real wait runs nothing real.
const TIMEOUT: &str = "shift 3; exec \"$@\"";

/// A Linux machine with apt: the tools the script runs before and during
/// an apt install. Each one that would change the system says `STUB <name>`
/// on stderr, so the order of what the script says and what it runs shows.
fn apt_machine() -> Machine {
    let mut machine = Machine::bare()
        .with_tool("id", "0")
        .with_tool("uname", "x86_64")
        .with("curl", "echo \"STUB curl $*\" >&2; echo key")
        .with(
            "tee",
            "echo \"STUB tee $*\" >&2; while IFS= read -r line; do :; done",
        );
    for name in ["sudo", "apt-get", "pacman", "dnf"] {
        machine = machine.with(name, &format!("echo \"STUB {name} $*\" >&2"));
    }
    machine
}

/// This machine's own `name`, if it has one.
fn real(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// This machine's own `name`, or none, with a note that the test stops
/// there. Linux has each tool asked for here, from coreutils, so on Linux a
/// missing one fails the test instead.
fn real_or_skip(name: &str) -> Option<PathBuf> {
    let found = real(name);
    assert!(
        found.is_some() || !cfg!(target_os = "linux"),
        "Linux has {name}, from coreutils"
    );
    if found.is_none() {
        eprintln!("skipped: this machine has no {name}");
    }
    found
}

/// Put this machine's `tool` on `machine`'s `PATH`, under its own name.
fn with_real(machine: Machine, tool: &Path) -> Machine {
    let name = tool.file_name().expect("a tool has a name");
    std::os::unix::fs::symlink(tool, machine.path().join(name)).expect("a link to the real tool");
    machine
}

/// A script at `dir/name` that records its call in `machine`'s record and
/// then runs `body`.
fn stub(machine: &Machine, dir: &Path, name: &str, body: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("a directory for it");
    let path = dir.join(name);
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"{} $*\" >> '{}'\n{body}\n",
        path.display(),
        machine.record().display()
    );
    std::fs::write(&path, script).expect("a stand-in is written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("a stand-in is made executable");
    path
}

/// The shells the script is run with: `sh`, and where they are installed
/// `dash`, Debian's and Ubuntu's `sh`, and `bash`, Arch's and Fedora's.
const SHELLS: &[&str] = &["/bin/sh", "/bin/dash", "/bin/bash"];

/// How the script reaches the shell.
#[derive(Clone, Copy)]
enum Fed {
    /// `sh install.sh`, with no input.
    AsAFile,
    /// `curl … | sh`: the script is the shell's stdin.
    OnStdin,
}

/// [`install_fed`], as `sh install.sh`.
fn install(packaged: &Path, path: &[&Path]) -> Output {
    install_fed(packaged, path, Fed::AsAFile)
}

/// Run a copy of `install.sh` whose packaged directory is `packaged`, with
/// `path` as its whole `PATH`, in an empty directory, under each of
/// [`SHELLS`] there is. Every shell must say the same before it installs
/// (the stand-ins in a pipeline may answer in either order); the first
/// one's run is returned.
fn install_fed(packaged: &Path, path: &[&Path], fed: Fed) -> Output {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/install.sh");
    let text = std::fs::read_to_string(&source).expect("install.sh is read");
    assert_eq!(
        text.matches(PACKAGED).count(),
        1,
        "install.sh names {PACKAGED:?} once"
    );
    let cwd = tempfile::tempdir().expect("an empty directory to run in");
    let copy = cwd.path().join("install.sh");
    let text = text.replace(PACKAGED, &format!("packaged='{}'\n", packaged.display()));
    std::fs::write(&copy, &text).expect("the copy is written");
    let run = |shell: &str| {
        let mut command = Command::new(shell);
        command
            .current_dir(cwd.path())
            .env_clear()
            .env("PATH", std::env::join_paths(path).expect("a PATH"))
            .env("HOME", cwd.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match fed {
            Fed::AsAFile => command.arg(&copy).stdin(Stdio::null()).output(),
            Fed::OnStdin => {
                let mut child = command.stdin(Stdio::piped()).spawn().expect("a shell");
                let mut stdin = child.stdin.take().expect("its stdin");
                let script = text.clone();
                // A shell that ends early closes the pipe; what it says
                // shows that, so a failed write is not looked at.
                let writer = std::thread::spawn(move || {
                    let _ = stdin.write_all(script.as_bytes());
                });
                let out = child.wait_with_output();
                writer.join().expect("the script is written");
                out
            }
        }
        .expect("install.sh runs")
    };
    let mut runs = SHELLS
        .iter()
        .filter(|shell| Path::new(shell).exists())
        .map(|shell| (*shell, run(shell)));
    let (first_shell, first) = runs.next().expect("/bin/sh is here");
    for (shell, out) in runs {
        assert_eq!(
            (out.status.code(), said_before_installing(&out)),
            (first.status.code(), said_before_installing(&first)),
            "{shell} and {first_shell} say the same"
        );
    }
    first
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// What the script said before its first package-manager step, which must
/// come.
fn said_before_installing(out: &Output) -> String {
    let err = stderr(out);
    let (before, _) = err
        .split_once("STUB ")
        .unwrap_or_else(|| panic!("a package-manager step ran: {err}"));
    before.to_owned()
}

/// The install went ahead as it does on a machine with no gglib: it ended
/// well, installed with apt and never asked `sudo` for anything.
fn assert_installed(machine: &Machine, out: &Output) {
    assert_eq!(out.status.code(), Some(0), "{}", stderr(out));
    assert!(
        stderr(out).contains("STUB apt-get install"),
        "{}",
        stderr(out)
    );
    let calls = machine.calls();
    assert!(
        !calls.iter().any(|call| ["sudo ", "pacman ", "dnf "]
            .iter()
            .any(|n| call.starts_with(n))),
        "{calls:?}"
    );
}

/// What `machine`'s record says was asked of the stand-in at `path`.
fn asked_of(machine: &Machine, path: &Path) -> Vec<String> {
    let prefix = format!("{} ", path.display());
    machine
        .calls()
        .iter()
        .filter_map(|call| call.strip_prefix(&prefix).map(str::to_owned))
        .collect()
}

#[test]
fn each_gglib_on_path_and_the_app_are_named_before_anything_is_installed() {
    let _guard = one_at_a_time();
    let machine = apt_machine().with("timeout", TIMEOUT);
    let root = tempfile::tempdir().expect("a directory for the copies");
    let (early, packaged, late) = (
        root.path().join("cargo/bin"),
        root.path().join("usr/bin"),
        root.path().join("opt/bin"),
    );
    let ahead = stub(
        &machine,
        &early,
        "gglib",
        "echo 'gglib 0.30.0 (a1b2c3d4e5f6)'",
    );
    let installed = stub(&machine, &packaged, "gglib", "echo 'gglib 0.34.0'");
    let behind = stub(&machine, &late, "gglib", "echo 'gglib 0.33.0-rc.1'");
    let app = stub(&machine, &packaged, "gglib-gui", "exit 0");
    let bin = machine.path();

    let out = install(&packaged, &[&early, &bin, &packaged, &late, &early]);

    assert_installed(&machine, &out);
    let said = said_before_installing(&out);
    let expected = [
        "gglib: already on PATH, in the order the shell tries them:".to_owned(),
        format!("gglib:   {} (0.30.0)", ahead.display()),
        format!("gglib:   {} (0.34.0)", installed.display()),
        format!("gglib:   {} (0.33.0-rc.1)", behind.display()),
        "gglib: the desktop app is already installed:".to_owned(),
        format!("gglib:   {}", app.display()),
        format!(
            "gglib: {} comes before {} on PATH, so `gglib` will still run it, not the {}/gglib this installs.",
            ahead.display(),
            packaged.display(),
            packaged.display()
        ),
        "gglib: every gglib here, and the desktop app, uses the same data folder.".to_owned(),
    ];
    assert_eq!(
        said.lines().collect::<Vec<_>>(),
        expected,
        "{}",
        stderr(&out)
    );
    // Each copy was asked its version and nothing else, once per shell, and
    // the app was never run.
    for copy in [&ahead, &installed, &behind] {
        let asked = asked_of(&machine, copy);
        assert!(!asked.is_empty(), "{copy:?} was asked");
        assert!(
            asked.iter().all(|args| args == "--version"),
            "{copy:?} was asked only its version: {asked:?}"
        );
    }
    assert_eq!(asked_of(&machine, &app), Vec::<String>::new());
    assert!(
        machine
            .calls()
            .contains(&format!("timeout -s KILL 2 {} --version", ahead.display())),
        "{:?}",
        machine.calls()
    );
}

#[test]
fn a_machine_with_no_gglib_hears_nothing_more() {
    let _guard = one_at_a_time();
    let machine = apt_machine().with("timeout", TIMEOUT);
    let root = tempfile::tempdir().expect("a packaged directory");
    let bin = machine.path();

    let out = install(root.path(), &[&bin, root.path()]);

    assert_installed(&machine, &out);
    assert_eq!(said_before_installing(&out), "");
}

#[test]
fn the_packaged_copy_first_on_path_runs_ahead_of_the_rest() {
    let _guard = one_at_a_time();
    let machine = apt_machine().with("timeout", TIMEOUT);
    let root = tempfile::tempdir().expect("a directory for the copies");
    let (packaged, late) = (root.path().join("usr/bin"), root.path().join("opt/bin"));
    let installed = stub(&machine, &packaged, "gglib", "echo 'gglib 0.34.0'");
    let behind = stub(&machine, &late, "gglib", "echo 'gglib 0.33.0'");
    let bin = machine.path();

    let out = install(&packaged, &[&packaged, &bin, &late]);

    assert_installed(&machine, &out);
    let said = said_before_installing(&out);
    assert_eq!(
        said.lines().collect::<Vec<_>>(),
        [
            "gglib: already on PATH, in the order the shell tries them:".to_owned(),
            format!("gglib:   {} (0.34.0)", installed.display()),
            format!("gglib:   {} (0.33.0)", behind.display()),
            format!(
                "gglib: {}/gglib, which this installs, comes first on PATH, so `gglib` will run it, not the others listed.",
                packaged.display()
            ),
            "gglib: every gglib here, and the desktop app, uses the same data folder.".to_owned(),
        ],
        "{}",
        stderr(&out)
    );
}

#[test]
fn without_timeout_to_bound_the_wait_no_copy_is_run() {
    let _guard = one_at_a_time();
    let machine = apt_machine();
    let root = tempfile::tempdir().expect("a directory for the copies");
    let (packaged, early) = (root.path().join("usr/bin"), root.path().join("cargo/bin"));
    let ahead = stub(&machine, &early, "gglib", "echo 'gglib 0.30.0'");
    let bin = machine.path();

    // The packaged directory is not on PATH at all.
    let out = install(&packaged, &[&early, &bin]);

    assert_installed(&machine, &out);
    let said = said_before_installing(&out);
    assert_eq!(
        said.lines().collect::<Vec<_>>(),
        [
            "gglib: already on PATH, in the order the shell tries them:".to_owned(),
            format!("gglib:   {} (version unknown)", ahead.display()),
            format!(
                "gglib: {} is not on PATH, so the gglib this installs runs only as {}/gglib; `gglib` will still run {}.",
                packaged.display(),
                packaged.display(),
                ahead.display()
            ),
            "gglib: every gglib here, and the desktop app, uses the same data folder.".to_owned(),
        ],
        "{}",
        stderr(&out)
    );
    assert_eq!(
        asked_of(&machine, &ahead),
        Vec::<String>::new(),
        "no copy is run"
    );
}

/// `curl … | sh`: the script is the shell's stdin, and a copy that reads
/// its own stdin is given none of it, so the rest of the script still runs.
#[test]
fn piped_into_sh_a_copy_that_reads_stdin_takes_none_of_the_script() {
    let _guard = one_at_a_time();
    let machine = apt_machine().with("timeout", TIMEOUT);
    let root = tempfile::tempdir().expect("a directory for the copies");
    let (packaged, early) = (root.path().join("usr/bin"), root.path().join("cargo/bin"));
    std::fs::create_dir_all(&packaged).expect("the packaged directory");
    // Its version's last number says whether it was given a line to read.
    let reads = stub(
        &machine,
        &early,
        "gglib",
        "n=0\nwhile IFS= read -r line; do n=1; done\necho \"gglib 0.30.$n\"",
    );
    let bin = machine.path();

    let out = install_fed(&packaged, &[&early, &bin, &packaged], Fed::OnStdin);

    assert_installed(&machine, &out);
    assert_eq!(
        said_before_installing(&out).lines().collect::<Vec<_>>(),
        [
            "gglib: already on PATH, in the order the shell tries them:".to_owned(),
            format!("gglib:   {} (0.30.0)", reads.display()),
            format!(
                "gglib: {} comes before {} on PATH, so `gglib` will still run it, not the {}/gglib this installs.",
                reads.display(),
                packaged.display(),
                packaged.display()
            ),
            "gglib: every gglib here, and the desktop app, uses the same data folder.".to_owned(),
        ],
        "{}",
        stderr(&out)
    );
}

/// With this machine's own `timeout`: a copy that hangs, ignoring the
/// polite signal and holding its output open from a child, is killed when
/// its two seconds are up. It, a copy that fails and one that prints
/// something else all read as `version unknown`, the odd `PATH` entries
/// around them are passed over, and the install goes ahead and ends well.
#[test]
fn a_copy_that_hangs_or_fails_reads_as_unknown_and_stops_nothing() {
    let _guard = one_at_a_time();
    let (Some(timeout), Some(sleep)) = (real_or_skip("timeout"), real_or_skip("sleep")) else {
        return;
    };
    let machine = with_real(with_real(apt_machine(), &timeout), &sleep);
    let root = tempfile::tempdir().expect("a directory for the copies");
    let at = |rel: &str| root.path().join(rel);
    let packaged = at("usr/bin");
    let spaced = stub(
        &machine,
        &at("with space/bin"),
        "gglib",
        "echo 'gglib 0.30.0'",
    );
    let fails = stub(
        &machine,
        &at("fails"),
        "gglib",
        "echo 'gglib 0.31.0'; exit 3",
    );
    let noise = stub(
        &machine,
        &at("noise"),
        "gglib",
        "printf 'gglib 0.32.0\\033[31m\\n'",
    );
    let hangs = stub(&machine, &at("hangs"), "gglib", "trap '' TERM\nsleep 60");
    // A gglib that cannot run, and a directory of that name, are not copies.
    std::fs::create_dir_all(at("unrunnable")).expect("a directory");
    std::fs::write(at("unrunnable/gglib"), "#!/bin/sh\nexit 0\n").expect("a file with no x bit");
    std::fs::create_dir_all(at("folder/gglib")).expect("a directory named gglib");
    std::fs::create_dir_all(&packaged).expect("the packaged directory");
    let bin = machine.path();

    let started = Instant::now();
    let out = install(
        &packaged,
        &[
            &at("missing"),
            &at("with space/bin"),
            &at("unrunnable"),
            &at("folder"),
            &at("fails"),
            &at("noise"),
            &at("hangs"),
            &bin,
            &packaged,
        ],
    );

    assert_installed(&machine, &out);
    assert!(
        started.elapsed() < Duration::from_secs(40),
        "took {:?}",
        started.elapsed()
    );
    let said = said_before_installing(&out);
    assert_eq!(
        said.lines().collect::<Vec<_>>(),
        [
            "gglib: already on PATH, in the order the shell tries them:".to_owned(),
            format!("gglib:   {} (0.30.0)", spaced.display()),
            format!("gglib:   {} (version unknown)", fails.display()),
            format!("gglib:   {} (version unknown)", noise.display()),
            format!("gglib:   {} (version unknown)", hangs.display()),
            format!(
                "gglib: {} comes before {} on PATH, so `gglib` will still run it, not the {}/gglib this installs.",
                spaced.display(),
                packaged.display(),
                packaged.display()
            ),
            "gglib: every gglib here, and the desktop app, uses the same data folder.".to_owned(),
        ],
        "{}",
        stderr(&out)
    );
}

/// A second run, on a merged-/usr system where `/bin` links to `/usr/bin`:
/// the packaged copy and its app are each named once, under the first path
/// that reaches them, and nothing is said to shadow anything.
#[test]
fn a_second_run_names_the_packaged_copy_once_and_nothing_shadows_it() {
    let _guard = one_at_a_time();
    let machine = apt_machine().with("timeout", TIMEOUT);
    let root = tempfile::tempdir().expect("a directory for the copies");
    let packaged = root.path().join("usr/bin");
    stub(&machine, &packaged, "gglib", "echo 'gglib 0.35.0'");
    stub(&machine, &packaged, "gglib-gui", "exit 0");
    let merged = root.path().join("bin");
    std::os::unix::fs::symlink(&packaged, &merged).expect("/bin links to /usr/bin");
    let bin = machine.path();

    let out = install(&packaged, &[&merged, &bin, &packaged]);

    assert_installed(&machine, &out);
    assert_eq!(
        said_before_installing(&out).lines().collect::<Vec<_>>(),
        [
            "gglib: already on PATH, in the order the shell tries them:".to_owned(),
            format!("gglib:   {}/gglib (0.35.0)", merged.display()),
            "gglib: the desktop app is already installed:".to_owned(),
            format!("gglib:   {}/gglib-gui", merged.display()),
        ],
        "{}",
        stderr(&out)
    );
}

/// With this machine's own `realpath`: a link to the packaged file from a
/// directory earlier on `PATH` is the packaged copy, not another one.
#[test]
fn a_link_to_the_packaged_file_is_not_another_copy() {
    let _guard = one_at_a_time();
    let Some(realpath) = real_or_skip("realpath") else {
        return;
    };
    let machine = with_real(apt_machine().with("timeout", TIMEOUT), &realpath);
    let root = tempfile::tempdir().expect("a directory for the copies");
    let (packaged, local) = (root.path().join("usr/bin"), root.path().join("local/bin"));
    let installed = stub(&machine, &packaged, "gglib", "echo 'gglib 0.35.0'");
    std::fs::create_dir_all(&local).expect("a directory for the link");
    std::os::unix::fs::symlink(&installed, local.join("gglib")).expect("a link to the file");
    let bin = machine.path();

    let out = install(&packaged, &[&local, &bin, &packaged]);

    assert_installed(&machine, &out);
    assert_eq!(
        said_before_installing(&out).lines().collect::<Vec<_>>(),
        [
            "gglib: already on PATH, in the order the shell tries them:".to_owned(),
            format!("gglib:   {}/gglib (0.35.0)", local.display()),
        ],
        "{}",
        stderr(&out)
    );
}
