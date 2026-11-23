//! The llama.cpp commands that ask before they act.
//!
//! The end of input is a no to every one of them. Each runs over a data root
//! the test laid out, and its `PATH` holds nothing or only stand-in tools, so
//! a command that went ahead regardless would find no real git and no
//! compiler to go ahead with. Two are told yes: an uninstall, which removes
//! what the test laid out, and a source install, which meets a stand-in git
//! that clones nothing and fails, which is where an install stops.

#[path = "support/stand_in_tools.rs"]
mod stand_in_tools;

use gglib_core::utils::process::cmd;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// `gglib <args>` over `root` with nothing on its `PATH`, and with `typed` on
/// its standard input, or the end of input when there is none.
fn gglib(root: &Path, args: &[&str], typed: Option<&str>) -> Output {
    let nothing = tempfile::tempdir().expect("an empty PATH");
    gglib_on(nothing.path(), root, args, typed)
}

/// `gglib <args>` over `root`, run in `dir`, with `path` as its whole `PATH`
/// and its output captured.
fn gglib_command(path: &Path, root: &Path, dir: &Path, args: &[&str]) -> Command {
    let mut command = cmd(env!("CARGO_BIN_EXE_gglib"));
    command
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", path)
        .env("HOME", root)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_RESOURCE_DIR", root)
        // A command that reached for GitHub would meet a proxy nobody is at.
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// [`gglib`] with `path` as its whole `PATH`.
fn gglib_on(path: &Path, root: &Path, args: &[&str], typed: Option<&str>) -> Output {
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let mut child = gglib_command(path, root, elsewhere.path(), args)
        .stdin(typed.map_or_else(Stdio::null, |_| Stdio::piped()))
        .spawn()
        .expect("starting gglib");
    if let Some(typed) = typed {
        let mut stdin = child.stdin.take().expect("its standard input");
        stdin.write_all(typed.as_bytes()).expect("typing at it");
    }
    child.wait_with_output().expect("running gglib")
}

fn text(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A `llama-server`, its checkout and its record, where gglib keeps them.
fn install(root: &Path) {
    std::fs::create_dir_all(root.join(".llama/bin")).unwrap();
    std::fs::write(root.join(".llama/bin/llama-server"), "not a program").unwrap();
    std::fs::create_dir_all(root.join(".llama/llama.cpp")).unwrap();
    std::fs::create_dir_all(root.join(".llama/llama.cpp.new")).unwrap();
    std::fs::write(root.join(".llama/llama-config.json"), "{}").unwrap();
}

/// `gglib serve` once took the end of input for a yes and started a build
/// nobody had agreed to. With no terminal it now asks nothing at all.
#[test]
fn serve_with_no_llama_and_nobody_to_ask_installs_nothing() {
    let root = tempfile::tempdir().unwrap();

    let out = gglib(root.path(), &["serve", "any-model"], None);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        stdout.contains("llama.cpp is not installed; chat models need its llama-server."),
        "{}",
        text(&out)
    );
    assert!(
        !stdout.contains("(Y/n)"),
        "nobody was asked: {}",
        text(&out)
    );
    assert!(
        !stdout.contains("Checking build dependencies"),
        "no build was started: {}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(NO_TERMINAL_REFUSAL),
        "{}",
        text(&out)
    );
    assert!(!root.path().join(".llama").exists(), "{}", text(&out));
}

/// What a command that needs llama.cpp says when it is missing and there is
/// no terminal to ask.
const NO_TERMINAL_REFUSAL: &str = "Not running in a terminal, so there is nobody to ask whether \
     to install llama.cpp. llama.cpp is required to run this command. Run 'gglib config llama \
     install' manually.";

/// A stdin that is a pipe nobody writes to and nobody closes, as a
/// backgrounded shell gives, is not read: the offer once waited on it
/// forever, and `gglib model load` hung before asking the daemon anything.
#[test]
fn serve_with_a_silent_open_stdin_refuses_at_once() {
    let root = tempfile::tempdir().unwrap();
    let nothing = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let mut child = gglib_command(
        nothing.path(),
        root.path(),
        elsewhere.path(),
        &["serve", "any-model"],
    )
    .stdin(Stdio::piped())
    .spawn()
    .expect("starting gglib");
    // Held open, and never written to, until gglib is done.
    let stdin = child.stdin.take().expect("its standard input");

    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().expect("polling gglib").is_none() {
        if Instant::now() >= deadline {
            child.kill().expect("stopping gglib");
            let out = child.wait_with_output().expect("reaping gglib");
            panic!("gglib waited on a stdin nobody writes to: {}", text(&out));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(stdin);
    let out = child.wait_with_output().expect("running gglib");

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(NO_TERMINAL_REFUSAL),
        "{}",
        text(&out)
    );
    assert!(!root.path().join(".llama").exists(), "{}", text(&out));
}

/// `gglib up` stops at its engines step when the installed llama-server
/// does not work, saying so and why, rather than going on to a model that
/// could not be served (#1959 A1). Nothing is installed or asked.
#[test]
fn up_stops_at_an_installed_llama_server_that_does_not_work() {
    let root = tempfile::tempdir().unwrap();
    // Joined a name at a time: the path is compared with what gglib prints,
    // and a `/` inside one name is not what Windows prints.
    let bin = root.path().join(".llama").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let server = bin.join(format!("llama-server{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&server, "not a program").unwrap();

    let out = gglib(root.path(), &["up"], None);

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("llama.cpp installed, not working"),
        "{}",
        text(&out)
    );
    assert!(
        !stdout.contains("(Y/n)"),
        "nobody was asked: {}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&server.display().to_string()),
        "the reason names the binary: {}",
        text(&out)
    );
}

#[test]
fn uninstall_with_nothing_installed_says_so_and_asks_nothing() {
    let root = tempfile::tempdir().unwrap();

    let out = gglib(root.path(), &["config", "llama", "uninstall"], None);

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "llama.cpp is not installed.\n"
    );
}

/// With no terminal, the question is not asked and its input not read, a
/// yes waiting in a pipe included: the uninstall is refused naming `--yes`,
/// and everything stays.
#[test]
fn uninstall_with_nobody_to_ask_removes_nothing_and_names_the_flag() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    for typed in [None, Some("yes\n")] {
        let out = gglib(root.path(), &["config", "llama", "uninstall"], typed);

        assert_eq!(out.status.code(), Some(1), "{}", text(&out));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "{}", text(&out));
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            "Error: There is no terminal to ask, so llama.cpp was not removed: run it again with \
             --yes to remove it.\n"
        );
        assert!(root.path().join(".llama/bin/llama-server").exists());
        assert!(root.path().join(".llama/llama-config.json").exists());
    }
}

/// `--yes` is the answer given up front: nothing is asked, so with nobody
/// to ask it still removes everything.
#[test]
fn uninstall_with_yes_asks_nothing_and_removes_everything() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(
        root.path(),
        &["config", "llama", "uninstall", "--yes"],
        None,
    );

    assert!(out.status.success(), "{}", text(&out));
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("Continue?"),
        "{}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).ends_with("llama.cpp uninstalled successfully.\n"),
        "{}",
        text(&out)
    );
    assert!(
        !root.path().join(".llama").exists(),
        "the folder it emptied"
    );
}

/// A source build is refused before anything is cloned when the tools that
/// build it are missing, with how to install them.
#[test]
fn a_source_install_without_the_build_tools_is_refused_and_says_how_to_get_them() {
    let root = tempfile::tempdir().unwrap();

    let out = gglib(
        root.path(),
        &["config", "llama", "install", "--build"],
        None,
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        stdout.starts_with(
            "Checking build dependencies...\n\
             ✗ git not found\n\
             ✗ cmake not found\n\
             ✗ C++ compiler not found\n\
             \n\
             Missing dependencies detected. Please install:\n"
        ),
        "{}",
        text(&out)
    );
    assert!(
        stdout.ends_with("After installing, run 'gglib config llama install' again.\n"),
        "{}",
        text(&out)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        "Error: Missing required build dependencies",
        "{}",
        text(&out)
    );
}

/// What a build detects on the stand-in machine: Metal on Apple silicon,
/// and elsewhere CUDA, from the stand-in `nvcc`.
const DETECTED: &str = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    "Metal"
} else {
    "CUDA"
};

/// The stand-in toolkit a build detects off Apple silicon.
const NVCC: &str = "Cuda compilation tools, release 12.4, V12.4.131";

/// With the tools there, a source install says what it found and what it
/// will do, and would ask. There is no terminal to ask, so nothing is
/// cloned, and the command says so, names `--yes` and exits with an error.
#[test]
fn a_source_install_with_the_build_tools_asks_before_it_clones_anything() {
    let root = tempfile::tempdir().unwrap();
    let machine = stand_in_tools::Machine::bare()
        .with_tool("git", "git version 2.43.0")
        .with_tool("cmake", "cmake version 3.28.1")
        .with_tool("g++", "g++ (GCC) 14.2.1 20240910")
        .with_tool("nvcc", NVCC);

    let out = gglib_on(
        &machine.path(),
        root.path(),
        &["config", "llama", "install", "--build"],
        None,
    );

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "Checking build dependencies...\n\
             ✓ git (version 2.43.0)\n\
             ✓ cmake (version 3.28.1)\n\
             ✓ C++ compiler g++ (g++ (GCC) 14.2.1 20240910)\n\
             \n\
             Selected acceleration: {DETECTED}\n\
             \n\
             Pre-flight check:\n\
             ✓ Build dependencies installed\n\
             ✓ Detected: {DETECTED}\n\
             \n\
             This will:\n\
             \x20 1. Clone llama.cpp repository (~150 MB)\n\
             \x20 2. Configure with CMake ({DETECTED} enabled)\n\
             \x20 3. Compile llama-server (~3-5 minutes)\n\
             \x20 4. Install to {}\n\
             \n",
            root.path().join(".llama").join("bin").display()
        ),
        "{}",
        text(&out)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        "Error: There is no terminal to ask, so nothing was built: run it again with --yes to \
         build it.",
        "{}",
        text(&out)
    );
    assert!(
        !machine.calls().iter().any(|call| call.contains("clone")),
        "nothing was cloned: {:?}",
        machine.calls()
    );
}

/// A clone's own output is piped into the build's log lines, and they are
/// drawn with the spinner over the clone. Off a terminal neither is drawn, so
/// nothing git says of the clone is left on stdout.
#[test]
fn a_source_install_that_is_agreed_to_clones_under_its_spinner() {
    let root = tempfile::tempdir().unwrap();
    let machine = stand_in_tools::Machine::bare()
        .with(
            "git",
            "when clone eprint Cloning into 'a stand-in'...\n\
             when clone exit 1\n\
             print git version 2.43.0",
        )
        .with_tool("cmake", "cmake version 3.28.1")
        .with_tool("g++", "g++ (GCC) 14.2.1 20240910")
        .with_tool("nvcc", NVCC);

    let out = gglib_on(
        &machine.path(),
        root.path(),
        &["config", "llama", "install", "--build", "--yes"],
        None,
    );

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("Continue?"), "{}", text(&out));
    assert!(!stdout.contains("Cloning into"), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "Error: Failed to clone the llama.cpp repository\n",
        "{}",
        text(&out)
    );
    let calls = machine.calls();
    let cmake_ran = |c: &String| c.starts_with("cmake") && c != "cmake --version";
    assert!(
        calls.iter().any(|c| c.starts_with("git clone ")),
        "{calls:?}"
    );
    assert!(!calls.iter().any(cmake_ran), "nothing was built: {calls:?}");
}
