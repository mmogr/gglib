//! `gglib config <engine> install` and `update` over an install that needs
//! nothing: the pin, or a later release. Each runs over a data root the test
//! laid out, with nothing on its `PATH` and every proxy variable pointing at
//! a port nobody listens on, so a command that went ahead would find no git
//! and no network to go ahead with.

use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `gglib <args>` over `root`, with nothing on its `PATH` and the end of
/// input on its standard input.
fn gglib(root: &Path, args: &[&str]) -> Output {
    let nothing = tempfile::tempdir().expect("an empty PATH");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .current_dir(elsewhere.path())
        .env_clear()
        .env("PATH", nothing.path())
        .env("HOME", root)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_RESOURCE_DIR", root)
        // A command that reached for GitHub would meet a proxy nobody is at.
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("running gglib")
}

fn text(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A downloaded `server` of `release` under `dir` (`.llama` or `.sd`), with
/// the record a download writes in `record`.
fn downloaded(root: &Path, dir: &str, server: &str, record: &str, release: &str) {
    let home = root.join(dir);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::write(home.join("bin").join(server), "not a program").unwrap();
    std::fs::write(
        home.join(record),
        format!(
            r#"{{"version":"{release}","platform":"a platform","install_type":"prebuilt","installed_at":"2026-10-25T00:00:00+00:00"}}"#
        ),
    )
    .unwrap();
}

/// The pinned release, as the command reports it, from the status line.
fn pinned_in(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find(|line| line.starts_with("Pinned release: "))
        .unwrap_or_else(|| panic!("no pinned release in {}", text(out)));
    line["Pinned release: ".len()..]
        .split([',', '.'])
        .next()
        .unwrap()
        .to_owned()
}

/// The pin installed is nothing to do, for either engine, and either verb.
#[test]
fn the_pin_installed_is_nothing_to_install() {
    for (engine, dir, server, record) in [
        ("llama", ".llama", "llama-server", "llama-config.json"),
        ("sd", ".sd", "sd-server", "sd-config.json"),
    ] {
        // The pin is read from the command's own status of an empty root.
        let empty = tempfile::tempdir().unwrap();
        downloaded(empty.path(), dir, server, record, "b1");
        let pin = pinned_in(&gglib(empty.path(), &["config", engine, "status"]));

        for verb in ["install", "update"] {
            let root = tempfile::tempdir().unwrap();
            downloaded(root.path(), dir, server, record, &pin);

            let out = gglib(root.path(), &["config", engine, verb]);

            assert!(out.status.success(), "{engine} {verb}: {}", text(&out));
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                format!(
                    "Pinned release: {pin}, which is the one installed.\nUse --force to reinstall it.\n"
                ),
                "{engine} {verb}: {}",
                text(&out)
            );
        }
    }
}

/// A later release than the pin, as a trial of the next pin leaves, is never
/// replaced by the pin unless forced.
#[test]
fn a_later_release_is_kept() {
    let root = tempfile::tempdir().unwrap();
    downloaded(
        root.path(),
        ".llama",
        "llama-server",
        "llama-config.json",
        "b99999999",
    );

    let out = gglib(root.path(), &["config", "llama", "update"]);

    assert!(out.status.success(), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("The one installed, b99999999, is newer."),
        "{}",
        text(&out)
    );
    assert!(
        stdout.ends_with("Use --force to install the pinned release.\n"),
        "{}",
        text(&out)
    );
}
