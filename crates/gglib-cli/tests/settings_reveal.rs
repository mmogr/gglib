//! `gglib config settings reveal`, `show` and `set` against a database on
//! disk: a stored secret is printed by `reveal` alone (ADR 0027).
//!
//! The unit tests over `write_value` hand it buffers; this runs the command a
//! person types, so it also covers everything the binary prints before the
//! handler runs. `$(gglib config settings reveal proxy-api-key)` is only the
//! key if nothing else reaches stdout.
//!
//! Every run sets `RUST_LOG=trace`, so a tracing event anywhere in the run
//! is written to stderr, where these tests look for the key. Each also names
//! a daemon port nothing listens on, so no run can reach a daemon of the
//! machine's own.

use std::net::TcpListener;
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
            name: None,
        });
    });
    root
}

/// A loopback port nothing listens on once this returns.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("its address").port()
}

/// `gglib config settings <args>` with `root` as its data directory.
fn settings(root: &Path, args: &[&str]) -> (Output, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["config", "settings"])
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_DAEMON_PORT", closed_port().to_string())
        .env("RUST_LOG", "trace")
        .output()
        .expect("running `gglib config settings`");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out, stdout, stderr)
}

#[test]
fn reveal_prints_each_secret_alone_on_stdout_and_never_on_stderr() {
    let root = seeded(true);

    for (name, value) in [
        ("proxy-api-key", PROXY_KEY),
        ("remote-pairing.api-key", DEVICE_KEY),
    ] {
        let (out, stdout, stderr) = settings(root.path(), &["reveal", name]);

        assert!(out.status.success(), "{name}\nstderr: {stderr}");
        assert_eq!(stdout, format!("{value}\n"), "{name}\nstderr: {stderr}");
        assert!(
            !stderr.contains(PROXY_KEY) && !stderr.contains(DEVICE_KEY),
            "{name}: no key may reach stderr: {stderr}"
        );
        assert!(
            stderr.contains("secret"),
            "{name}: stderr says it is a secret: {stderr}"
        );
    }
}

#[test]
fn reveal_with_no_pairing_exits_1_and_prints_neither_key() {
    let root = seeded(false);

    let (out, stdout, stderr) = settings(root.path(), &["reveal", "remote-pairing.api-key"]);

    assert_eq!(out.status.code(), Some(1), "stderr: {stderr}");
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("`gglib remote join <ticket>-<code>`"),
        "the refusal names the command that pairs: {stderr}"
    );
    assert!(
        stderr.contains("`gglib config settings reveal proxy-api-key`"),
        "the refusal names the proxy's own key: {stderr}"
    );
    assert!(
        !stderr.contains(PROXY_KEY),
        "the proxy key is named, not printed: {stderr}"
    );
}

#[test]
fn reveal_refuses_a_key_that_is_not_a_secret() {
    let root = seeded(true);

    let (out, stdout, stderr) = settings(root.path(), &["reveal", "proxy-port"]);

    assert_eq!(out.status.code(), Some(2), "clap's refusal: {stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("proxy-api-key"), "{stderr}");
}

/// `show` says each secret is held and how to print it, and prints neither.
#[test]
fn show_prints_no_secret() {
    let root = seeded(true);

    let (out, stdout, stderr) = settings(root.path(), &["show"]);

    assert!(out.status.success(), "stderr: {stderr}");
    for key in [PROXY_KEY, DEVICE_KEY] {
        assert!(
            !stdout.contains(key) && !stderr.contains(key),
            "{key} printed:\n{stdout}\n{stderr}"
        );
    }
    assert!(
        stdout.contains("(set, not shown)  `gglib config settings reveal proxy-api-key`"),
        "{stdout}"
    );
}

/// `set` echoes what it wrote through the rows `show` prints, so the key it
/// was just given is not printed back; `reveal` then has it.
#[test]
fn set_does_not_print_the_key_it_was_given() {
    let root = seeded(false);
    let new_key = "set-by-hand-2e7b";

    let (out, stdout, stderr) = settings(root.path(), &["set", "--proxy-api-key", new_key]);

    assert!(out.status.success(), "stderr: {stderr}");
    assert!(
        !stdout.contains(new_key) && !stderr.contains(new_key),
        "printed:\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains("(set, not shown)"), "{stdout}");

    let (_, revealed, _) = settings(root.path(), &["reveal", "proxy-api-key"]);
    assert_eq!(revealed, format!("{new_key}\n"));
}
