//! `GGLIB_API_KEY` beside `--remote`: a key from the environment was not
//! typed, so the proxy clients do not refuse it as `--api-key`.
//!
//! clap reads the process's own environment, so this drives the binary with
//! the variable set rather than setting it in the test process, where every
//! parser test would see it. Nothing is paired and no daemon answers on the
//! port given, so a command that is not refused stops at the pairing.

use std::process::{Command, Output};

/// `gglib <args>` over an empty data folder, with `GGLIB_API_KEY` set and a
/// daemon port nothing listens on.
fn run(args: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("temp data dir");
    let nobody = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a loopback port")
        .port();
    Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .env("GGLIB_DATA_DIR", dir.path())
        .env("GGLIB_DAEMON_PORT", nobody.to_string())
        .env("GGLIB_API_KEY", "from-the-environment")
        .output()
        .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")))
}

#[test]
fn a_key_from_the_environment_is_not_a_typed_api_key() {
    for client in ["dashboard", "cache-clear"] {
        let out = run(&["proxy", client, "--remote"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("--api-key"), "{client}: {stderr}");
        assert!(stderr.contains("gglib remote join"), "{client}: {stderr}");

        let out = run(&["proxy", client, "--remote", "--api-key", "typed"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("so --api-key would name a different one; drop it"),
            "{client}: {stderr}"
        );
    }
}
