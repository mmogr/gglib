//! `gglib remote key` against a database on disk.
//!
//! The unit tests over `write_key` hand it buffers; this runs the command a
//! person types, so it also covers everything the binary prints before the
//! handler runs. `$(gglib remote key --show)` is only the key if nothing else
//! reaches stdout.
//!
//! Every run sets `RUST_LOG=trace`, so a tracing event anywhere in the run
//! is written to stderr, where these tests look for the key.

use std::path::Path;
use std::process::{Command, Output};

use gglib_core::RemotePairing;

#[path = "support/data_dir.rs"]
mod data_dir;

const DEVICE_KEY: &str = "device-key-5f3a9c";
const PROXY_KEY: &str = "proxy-key-81d0e4";

/// A data directory whose settings hold a proxy key and, when `paired`, a
/// pairing with `DEVICE_KEY`.
fn seeded(paired: bool) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp data dir");
    data_dir::write_settings(root.path(), |settings| {
        settings.proxy_api_key = Some(PROXY_KEY.to_owned());
        settings.remote_pairing = paired.then(|| RemotePairing {
            ticket: "pipeabc".to_owned(),
            api_key: DEVICE_KEY.to_owned(),
            default_model: None,
            port: Some(8180),
        });
    });
    root
}

/// `gglib remote key <args>` with `root` as its data directory.
fn remote_key(root: &Path, args: &[&str]) -> (Output, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["remote", "key"])
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .env("RUST_LOG", "trace")
        .output()
        .expect("running `gglib remote key`");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out, stdout, stderr)
}

#[test]
fn remote_key_show_prints_the_device_key_alone_on_stdout_and_never_on_stderr() {
    let root = seeded(true);

    let (out, stdout, stderr) = remote_key(root.path(), &["--show"]);

    assert!(
        out.status.success(),
        "`--show` must succeed\nstderr: {stderr}"
    );
    assert_eq!(stdout, format!("{DEVICE_KEY}\n"), "stderr: {stderr}");
    assert!(
        !stderr.contains(DEVICE_KEY),
        "the key must not reach stderr: {stderr}"
    );
    assert!(
        stderr.contains("secret"),
        "stderr must say it is a secret: {stderr}"
    );
}

#[test]
fn a_bare_remote_key_prints_nothing_on_stdout_and_exits_0() {
    let root = seeded(true);

    let (out, stdout, stderr) = remote_key(root.path(), &[]);

    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(stdout, "", "stderr: {stderr}");
    assert!(
        stderr.contains("`gglib remote key --show`"),
        "stderr must say how to print the key: {stderr}"
    );
    assert!(
        !stderr.contains(DEVICE_KEY),
        "the key must not reach stderr: {stderr}"
    );
}

#[test]
fn remote_key_with_no_pairing_exits_1_and_prints_neither_key() {
    let root = seeded(false);

    for args in [&["--show"][..], &[]] {
        let (out, stdout, stderr) = remote_key(root.path(), args);

        assert_eq!(out.status.code(), Some(1), "{args:?}\nstderr: {stderr}");
        assert_eq!(stdout, "", "{args:?}");
        assert!(
            stderr.contains("`gglib remote join <ticket>-<code>`"),
            "{args:?}: the refusal must name the command that pairs: {stderr}"
        );
        assert!(
            stderr.contains("`proxy-api-key`"),
            "{args:?}: the refusal must name the proxy's own key: {stderr}"
        );
        assert!(
            !stderr.contains(PROXY_KEY),
            "{args:?}: the proxy key is named, not printed: {stderr}"
        );
    }
}
