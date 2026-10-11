//! `gglib config sd status` and `uninstall`, run as the binary over a data
//! root the test laid out.
//!
//! Each run has nothing on its `PATH` and its own `GGLIB_DATA_DIR` and
//! `GGLIB_RESOURCE_DIR`, so `.sd/` is the test's and nothing is downloaded or
//! built. The end of input is a no; an uninstall told yes removes what the
//! test laid out.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `gglib <args>` over `root` with nothing on its `PATH`, and `typed` on its
/// standard input, or the end of input when there is none.
fn gglib(root: &Path, args: &[&str], typed: Option<&str>) -> Output {
    let nothing = tempfile::tempdir().expect("an empty PATH");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let mut child = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .current_dir(elsewhere.path())
        .env_clear()
        .env("PATH", nothing.path())
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

/// An `sd-server` that is not a program, and a download's record.
fn install(root: &Path) {
    std::fs::create_dir_all(root.join(".sd/bin")).unwrap();
    std::fs::write(root.join(".sd/bin/sd-server"), "not a program").unwrap();
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
        "Status: Not installed\n\n\
         Run 'gglib config sd install' to install stable-diffusion.cpp (master-948-228c707).\n"
    );
}

#[test]
fn status_of_a_download_reads_its_record() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(root.path(), &["config", "sd", "status"], None);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", text(&out));
    for line in [
        "Status: Installed",
        "Pre-built download:",
        "  Release: master-948-228c707",
        "  Platform: macOS (Metal)",
        // A file that is not a program answers no --version.
        "Binary version: sd-server did not answer --version",
    ] {
        assert!(
            stdout.lines().any(|l| l == line),
            "{line:?}\n{}",
            text(&out)
        );
    }
    let binary = root.path().join(".sd/bin/sd-server");
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

#[test]
fn uninstall_with_nobody_to_ask_removes_nothing() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(root.path(), &["config", "sd", "uninstall"], None);

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "This will remove stable-diffusion.cpp and sd-server. Continue? (y/N): \n\
         Uninstall cancelled.\n"
    );
    assert!(root.path().join(".sd/bin/sd-server").exists());
}

#[test]
fn uninstall_told_yes_removes_sd_whole_and_says_what_it_removed() {
    let root = tempfile::tempdir().unwrap();
    install(root.path());

    let out = gglib(root.path(), &["config", "sd", "uninstall"], Some("yes\n"));

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", text(&out));
    let removed = format!("\u{2713} Removed {}", root.path().join(".sd").display());
    assert!(stdout.lines().any(|l| l == removed), "{}", text(&out));
    assert!(
        stdout.ends_with("stable-diffusion.cpp uninstalled successfully.\n"),
        "{}",
        text(&out)
    );
    assert!(!root.path().join(".sd").exists());
}
