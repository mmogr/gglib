//! Unit tests for [`super`], against the settings store the CLI's bootstrap
//! wires over a database file.

use std::sync::{Arc, Barrier};
use std::time::Duration;

use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
use gglib_core::{Device, NoopEmitter, RemotePairing, RemoteServe, Settings, SettingsRepository};

use super::reset;

fn device(id: &str) -> Device {
    Device {
        id: id.to_owned(),
        label: None,
        joined_at: 1,
        redeemed_at: None,
        last_seen: None,
        peer: None,
    }
}

fn pairing() -> RemotePairing {
    RemotePairing {
        ticket: "pipeabc".to_owned(),
        api_key: "far-key".to_owned(),
        default_model: Some("qwen3".to_owned()),
        port: Some(8181),
    }
}

/// The settings store `CoreBootstrap` wires over `dir`'s database, with a
/// pool of its own: what `ctx.settings_repo` is in the binary.
async fn store(dir: &tempfile::TempDir) -> Arc<dyn SettingsRepository> {
    let models_dir = dir.path().join("models");
    std::fs::create_dir_all(&models_dir).expect("models dir");
    let config = BootstrapConfig {
        db_path: dir.path().join("gglib.db"),
        llama_server_path: "/nonexistent/llama-server".into(),
        models_dir,
        hf_token: None,
    };
    CoreBootstrap::build(config, Arc::new(NoopEmitter::new()))
        .await
        .expect("the database opens")
        .repos
        .settings
}

/// A machine that joined another, admits one device, serves remote access
/// with MCP on, holds a proxy key, and has moved one preference.
async fn seed(repo: &dyn SettingsRepository) {
    repo.modify(&|settings: &mut Settings| {
        settings.proxy_api_key = Some("proxy-key".to_owned());
        settings.remote_pairing = Some(pairing());
        settings.remote_enabled = Some(true);
        settings.remote_serve = Some(RemoteServe {
            allow_mcp: true,
            ..RemoteServe::default()
        });
        settings.remote_devices = Some(vec![device("dev-0a1b2c3d")]);
        settings.proxy_port = Some(9191);
        Ok(())
    })
    .await
    .expect("seeded");
}

/// Waits on `barrier` without holding a runtime worker.
async fn meet(barrier: &Arc<Barrier>) {
    let barrier = Arc::clone(barrier);
    tokio::task::spawn_blocking(move || {
        barrier.wait();
    })
    .await
    .expect("met");
}

#[tokio::test]
async fn a_reset_keeps_the_remote_record_and_the_proxy_key_in_the_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = store(&dir).await;
    seed(repo.as_ref()).await;

    reset(repo.as_ref()).await.expect("the reset is stored");

    let stored = repo.load().await.expect("load");
    assert_eq!(
        stored.proxy_port,
        Settings::with_defaults().proxy_port,
        "a preference is reset"
    );
    assert_eq!(stored.proxy_api_key.as_deref(), Some("proxy-key"));
    assert_eq!(stored.remote_pairing, Some(pairing()));
    assert_eq!(stored.remote_enabled, Some(true));
    assert!(stored.remote_serve.is_some_and(|serve| serve.allow_mcp));
    assert_eq!(stored.remote_devices, Some(vec![device("dev-0a1b2c3d")]));
}

/// A device the daemon records while the reset waits for the store is kept:
/// the reset reads the roster after that write commits, not before it.
///
/// The daemon's write is held between its read and its write; the reset is
/// given time to read and write if nothing stops it, and then the daemon's
/// write is let go.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_device_recorded_while_a_reset_waits_is_kept() {
    let dir = tempfile::tempdir().expect("tempdir");
    let daemon = store(&dir).await;
    let cli = store(&dir).await;
    seed(daemon.as_ref()).await;
    let entered = Arc::new(Barrier::new(2));
    let released = Arc::new(Barrier::new(2));

    let invite = tokio::spawn({
        let daemon = Arc::clone(&daemon);
        let (entered, released) = (Arc::clone(&entered), Arc::clone(&released));
        async move {
            daemon
                .modify(&|settings: &mut Settings| {
                    entered.wait();
                    released.wait();
                    settings
                        .remote_devices
                        .get_or_insert_with(Vec::new)
                        .push(device("dev-11112222"));
                    Ok(())
                })
                .await
        }
    });
    meet(&entered).await;

    let resetting = tokio::spawn(async move { reset(cli.as_ref()).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    meet(&released).await;

    invite
        .await
        .expect("joined")
        .expect("the device is recorded");
    resetting
        .await
        .expect("joined")
        .expect("the reset is stored");

    let stored = daemon.load().await.expect("load");
    let ids: Vec<String> = stored
        .remote_devices
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, ["dev-0a1b2c3d", "dev-11112222"]);
    assert_eq!(stored.proxy_port, Settings::with_defaults().proxy_port);
}
