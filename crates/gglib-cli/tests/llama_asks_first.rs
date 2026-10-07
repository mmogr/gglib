//! The llama.cpp commands that ask before they act.
//!
//! The end of input is a no to every one of them. Each runs over a data root
//! the test laid out, and its `PATH` holds nothing or only stand-in tools, so
//! a command that went ahead regardless would find no real git and no
//! compiler to go ahead with. Two are told yes: an uninstall, which removes
//! what the test laid out, and a source install, which meets a stand-in git
//! that clones nothing and fails, which is where an install stops.

#[cfg(unix)]
#[path = "support/stub_tools.rs"]
mod stub_tools;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `gglib <args>` over `root` with nothing on its `PATH`, and with `typed` on
/// its standard input, or the end of input when there is none.
fn gglib(root: &Path, args: &[&str], typed: Option<&str>) -> Output {
    let nothing = tempfile::tempdir().expect("an empty PATH");
    gglib_on(nothing.path(), root, args, typed)
}

/// [`gglib`] with `path` as its whole `PATH`.
fn gglib_on(path: &Path, root: &Path, args: &[&str], typed: Option<&str>) -> Output {
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let mut child = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .current_dir(elsewhere.path())
        .env_clear()
        .env("PATH", path)
        .env("HOME", root)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_RESOURCE_DIR", root)
        .stdin(typed.map_or_else(Stdio::null, |_| Stdio::piped()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
    std::fs::write(root.join(".llama/llama-config.json"), "{}").unwrap();
}

/// `gglib serve` once took the end of input for a yes and started a build
/// nobody had agreed to.
#[test]
fn serve_with_no_llama_and_nobody_to_ask_installs_nothing() {
    let root = tempfile::tempdir().unwrap();

    let out = gglib(root.path(), &["serve", "any-model"], None);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        stdout.contains("llama.cpp binaries not found."),
        "{}",
        text(&out)
    );
    assert!(
        stdout.contains("Would you like to install llama.cpp now? (Y/n): "),
        "{}",
        text(&out)
    );
    assert!(
        !stdout.contains("Checking build dependencies"),
        "no build was started: {}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(
            "llama.cpp is required to run this command. Run 'gglib config llama install' manually."
        ),
        "{}",
        text(&out)
    );
    assert!(!root.path().join(".llama").exists(), "{}", text(&out));
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

#[test]
fn uninstall_with_nobody_to_ask_removes_nothing() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(root.path(), &["config", "llama", "uninstall"], None);

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "This will remove llama.cpp and llama-server. Continue? (y/N): \nUninstall cancelled.\n"
    );
    assert!(root.path().join(".llama/bin/llama-server").exists());
    assert!(root.path().join(".llama/llama-config.json").exists());
}

/// The question is the one every command asks through, so `yes` is a yes
/// here as it is everywhere else.
#[test]
fn uninstall_told_yes_removes_the_binaries_the_checkout_and_the_record() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(
        root.path(),
        &["config", "llama", "uninstall"],
        Some("yes\n"),
    );

    assert!(out.status.success(), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).ends_with("llama.cpp uninstalled successfully.\n"),
        "{}",
        text(&out)
    );
    for gone in ["bin", "llama.cpp", "llama-config.json"] {
        assert!(!root.path().join(".llama").join(gone).exists(), "{gone}");
    }
}

/// A source build is refused before anything is cloned when the tools that
/// build it are missing, with how to install them.
#[test]
fn a_source_install_without_the_build_tools_is_refused_and_says_how_to_get_them() {
    let root = tempfile::tempdir().unwrap();

    let out = gglib(root.path(), &["config", "llama", "install", "--cuda"], None);

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

/// With the tools there, a source install says what it found and what it
/// will do, and asks. Nobody answers, so nothing is cloned.
#[cfg(unix)]
#[test]
fn a_source_install_with_the_build_tools_asks_before_it_clones_anything() {
    let _guard = stub_tools::one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    let machine = stub_tools::Machine::bare()
        .with_tool("git", "git version 2.43.0")
        .with_tool("cmake", "cmake version 3.28.1")
        .with_tool("g++", "g++ (GCC) 14.2.1 20240910");

    let out = gglib_on(
        &machine.path(),
        root.path(),
        &["config", "llama", "install", "--cuda"],
        None,
    );

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "Checking build dependencies...\n\
             ✓ git (version 2.43.0)\n\
             ✓ cmake (version 3.28.1)\n\
             ✓ C++ compiler g++ (g++ (GCC) 14.2.1 20240910)\n\
             \n\
             Selected acceleration: CUDA\n\
             \n\
             Pre-flight check:\n\
             ✓ Build dependencies installed\n\
             ✓ Detected: CUDA\n\
             \n\
             This will:\n\
             \x20 1. Clone llama.cpp repository (~150 MB)\n\
             \x20 2. Configure with CMake (CUDA enabled)\n\
             \x20 3. Compile llama-server (~3-5 minutes)\n\
             \x20 4. Install to {}\n\
             \n\
             Continue? (Y/n): \n\
             Installation cancelled.\n",
            root.path().join(".llama/bin").display()
        ),
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
#[cfg(unix)]
#[test]
fn a_source_install_that_is_agreed_to_clones_under_its_spinner() {
    let _guard = stub_tools::one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    let machine = stub_tools::Machine::bare()
        .with(
            "git",
            "case \"$*\" in\n\
             *clone*) echo \"Cloning into 'a stand-in'...\" >&2; exit 1 ;;\n\
             *) echo 'git version 2.43.0' ;;\n\
             esac",
        )
        .with_tool("cmake", "cmake version 3.28.1")
        .with_tool("g++", "g++ (GCC) 14.2.1 20240910");

    let out = gglib_on(
        &machine.path(),
        root.path(),
        &["config", "llama", "install", "--cuda"],
        Some("y\n"),
    );

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.ends_with("Continue? (Y/n): \n"), "{}", text(&out));
    assert!(!stdout.contains("Cloning into"), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "Error: Failed to clone llama.cpp repository\n",
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
