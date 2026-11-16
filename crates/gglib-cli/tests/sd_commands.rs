//! `gglib config sd status`, `uninstall` and a source `install` told no,
//! run as the binary over a data root the test laid out.
//!
//! Each run has nothing on its `PATH`, or only stand-in tools, and its own
//! `GGLIB_DATA_DIR` and `GGLIB_RESOURCE_DIR`, so `.sd/` is the test's and
//! nothing is downloaded or built. The end of input is a no; an uninstall
//! told yes removes what the test laid out.

#[cfg(unix)]
#[path = "support/stub_tools.rs"]
mod stub_tools;

use gglib_core::utils::process::cmd;
use std::io::Write;
use std::path::Path;
use std::process::{Output, Stdio};

/// `gglib <args>` over `root` with nothing on its `PATH`, and `typed` on its
/// standard input, or the end of input when there is none.
fn gglib(root: &Path, args: &[&str], typed: Option<&str>) -> Output {
    let nothing = tempfile::tempdir().expect("an empty PATH");
    gglib_on(nothing.path(), root, args, typed)
}

/// [`gglib`] with `path` as its whole `PATH`.
fn gglib_on(path: &Path, root: &Path, args: &[&str], typed: Option<&str>) -> Output {
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let mut child = cmd(env!("CARGO_BIN_EXE_gglib"))
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

/// Where gglib looks for `sd-server` under `root`, `.exe` on Windows.
fn sd_server(root: &Path) -> std::path::PathBuf {
    let file = format!("sd-server{}", std::env::consts::EXE_SUFFIX);
    root.join(".sd").join("bin").join(file)
}

/// An `sd-server` that is not a program, and a download's record.
fn install(root: &Path) {
    std::fs::create_dir_all(root.join(".sd").join("bin")).unwrap();
    std::fs::write(sd_server(root), "not a program").unwrap();
    std::fs::write(
        root.join(".sd/sd-config.json"),
        r#"{"install_type":"prebuilt","version":"master-948-228c707","platform":"macOS (Metal)","installed_at":"2026-10-10T01:02:03+00:00"}"#,
    )
    .unwrap();
}

#[test]
fn status_with_nothing_installed_says_how_to_install() {
    let root = tempfile::tempdir().unwrap();

    let out = gglib(root.path(), &["config", "sd", "status"], None);

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "Status: Not installed\n\n\
             Run 'gglib config sd install' to install stable-diffusion.cpp ({}).\n",
            gglib_runtime::sd::PINNED_SD_RELEASE
        )
    );
}

/// A download whose `sd-server` is not a program is installed and not
/// working, and says why, as `config llama status` does (#1959 B3); it once
/// read as installed.
#[test]
fn status_of_a_download_that_is_not_a_program_is_not_working() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(root.path(), &["config", "sd", "status"], None);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        stdout
            .lines()
            .any(|l| l == "Status: Installed, not working"),
        "{}",
        text(&out)
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("Health: \u{2717} Error - sd-server")),
        "{}",
        text(&out)
    );
    let binary = sd_server(root.path());
    assert!(
        stdout.contains(&format!("Binary: {}", binary.display())),
        "{}",
        text(&out)
    );
}

#[test]
fn uninstall_with_nothing_installed_says_so_and_asks_nothing() {
    let root = tempfile::tempdir().unwrap();

    let out = gglib(root.path(), &["config", "sd", "uninstall"], None);

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "stable-diffusion.cpp is not installed.\n"
    );
}

/// With no terminal, nothing is asked and nothing read, a yes waiting in a
/// pipe included: the uninstall is refused naming `--yes`.
#[test]
fn uninstall_with_nobody_to_ask_removes_nothing_and_names_the_flag() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    for typed in [None, Some("yes\n")] {
        let out = gglib(root.path(), &["config", "sd", "uninstall"], typed);

        assert_eq!(out.status.code(), Some(1), "{}", text(&out));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "{}", text(&out));
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            "Error: There is no terminal to ask, so stable-diffusion.cpp was not removed: run it \
             again with --yes to remove it.\n"
        );
        assert!(sd_server(root.path()).exists());
    }
}

#[test]
fn uninstall_with_yes_removes_what_sd_installed_and_says_what_it_removed() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(root.path(), &["config", "sd", "uninstall", "--yes"], None);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", text(&out));
    for name in ["bin", "sd-config.json"] {
        let removed = format!(
            "\u{2713} Removed {}",
            root.path().join(".sd").join(name).display()
        );
        assert!(stdout.lines().any(|l| l == removed), "{}", text(&out));
    }
    assert!(
        stdout.ends_with("stable-diffusion.cpp uninstalled successfully.\n"),
        "{}",
        text(&out)
    );
    assert!(!root.path().join(".sd").exists());
}

/// `-y` is the answer given up front: nothing is asked, so with nobody to
/// ask it still removes everything.
#[test]
fn uninstall_with_yes_asks_nothing_and_removes_everything() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(root.path(), &["config", "sd", "uninstall", "-y"], None);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!stdout.contains("Continue?"), "{}", text(&out));
    assert!(
        stdout.ends_with("stable-diffusion.cpp uninstalled successfully.\n"),
        "{}",
        text(&out)
    );
    assert!(!root.path().join(".sd").exists());
}

/// A source install with nobody to ask is refused with an error naming
/// `--yes`, so a script that ran it knows nothing was installed, and nothing
/// is cloned; a no waiting in a pipe is not read. The stand-in `nvcc` gives a
/// machine with no Metal a CUDA build to ask about.
#[cfg(unix)]
#[test]
fn a_source_install_with_nobody_to_ask_is_refused_and_clones_nothing() {
    let _guard = stub_tools::one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    let machine = stub_tools::Machine::bare()
        .with_tool("git", "git version 2.43.0")
        .with_tool("cmake", "cmake version 3.28.1")
        .with_tool("g++", "g++ (GCC) 14.2.1 20240910")
        .with_tool("nvcc", "Cuda compilation tools, release 12.4, V12.4.131");

    let out = gglib_on(
        &machine.path(),
        root.path(),
        &["config", "sd", "install", "--build"],
        Some("n\n"),
    );

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("This will:\n  1. Clone stable-diffusion.cpp and its submodules\n"),
        "{}",
        text(&out)
    );
    assert!(!stdout.contains("Continue?"), "{}", text(&out));
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
    assert!(!root.path().join(".sd").exists(), "{}", text(&out));
}
