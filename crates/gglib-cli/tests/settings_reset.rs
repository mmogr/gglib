//! `gglib config settings reset --force` against a database on disk.
//!
//! The unit tests over `reset` hand it a store they built; this runs the
//! command a person types, against the database the binary opens under
//! `GGLIB_DATA_DIR`.

use std::process::Command;

use gglib_core::{Device, RemotePairing, RemoteServe, Settings};

#[path = "support/data_dir.rs"]
mod data_dir;

#[test]
fn a_reset_keeps_the_pairing_the_device_roster_and_the_proxy_key() {
    let root = tempfile::tempdir().expect("temp data dir");
    data_dir::write_settings(root.path(), |settings| {
        settings.proxy_api_key = Some("proxy-key".to_owned());
        settings.remote_pairing = Some(RemotePairing {
            ticket: "pipeabc".to_owned(),
            api_key: "far-key".to_owned(),
            default_model: Some("qwen3".to_owned()),
            port: Some(8181),
        });
        settings.remote_enabled = Some(true);
        settings.remote_serve = Some(RemoteServe {
            allow_mcp: true,
            ..RemoteServe::default()
        });
        settings.remote_devices = Some(vec![Device {
            id: "dev-0a1b2c3d".to_owned(),
            label: Some("phone".to_owned()),
            joined_at: 1,
            redeemed_at: Some(2),
            last_seen: None,
            peer: None,
        }]);
        settings.proxy_port = Some(9191);
        settings.default_context_size = Some(4096);
    });
    let before = data_dir::read_settings(root.path());

    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["config", "settings", "reset", "--force"])
        .env("GGLIB_DATA_DIR", root.path())
        .output()
        .expect("running `gglib config settings reset --force`");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the reset must succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("Kept: the machine this one joined"),
        "the reset must say what it kept, got: {stdout}"
    );

    assert_eq!(
        data_dir::read_settings(root.path()),
        Settings {
            proxy_api_key: before.proxy_api_key,
            remote_pairing: before.remote_pairing,
            remote_enabled: before.remote_enabled,
            remote_serve: before.remote_serve,
            remote_devices: before.remote_devices,
            ..Settings::with_defaults()
        }
    );
}
