//! `forget` drops the device's runs, and only once no key admits it, even
//! when a later step of `forget` fails.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::access::load_device_keys;
use gglib_core::domain::runs::{RunInfo, RunList};
use gglib_core::ports::{
    AppEventEmitter, Created, RepositoryError, RunEvents, RunScope, RunsError, RunsPort,
    SettingsRepository,
};
use gglib_core::{Settings, SettingsUpdate};
use gglib_db::{CoreFactory, setup_test_database};
use serde_json::Value;

use super::super::RemoteOps;
use super::super::device_keys::{read_keys, write_keys};
use crate::runs::RunRegistry;
use crate::runs::test_executor::{Cmd, body, next, registry};
use crate::test_support::{RecordingEmitter, test_core_and_proxy_over};
use crate::test_support_remote::{scratch_device_keys, test_remote_ops};

const DEVICE: &str = "dev-11112222";

/// The real registry, noting whether the device still had a key each time it
/// was told to forget it.
#[derive(Debug)]
struct Watched {
    inner: RunRegistry,
    keys: PathBuf,
    key_held_at_forget: Mutex<Vec<bool>>,
}

impl RunsPort for Watched {
    fn create(&self, scope: RunScope, id: &str, body: Value) -> Result<Created, RunsError> {
        self.inner.create(scope, id, body)
    }
    fn list(&self, scope: &RunScope) -> RunList {
        self.inner.list(scope)
    }
    fn get(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError> {
        self.inner.get(scope, id)
    }
    fn events(&self, scope: &RunScope, id: &str, after: u32) -> Result<RunEvents, RunsError> {
        self.inner.events(scope, id, after)
    }
    fn cancel(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError> {
        self.inner.cancel(scope, id)
    }
    fn forget_device(&self, device: &str) -> usize {
        let held = load_device_keys(&self.keys)
            .expect("the key file reads")
            .contains_key(device);
        self.key_held_at_forget.lock().unwrap().push(held);
        self.inner.forget_device(device)
    }
}

/// A device with a key, one run of its own with an open reader, and one run
/// of this machine's, all behind `ops`' proxy.
async fn a_device_with_a_run(ops: &RemoteOps) -> (Arc<Watched>, RunEvents) {
    write_keys(
        ops,
        &std::iter::once((DEVICE.to_owned(), "sk-zzq-held".to_owned())).collect(),
    )
    .expect("the device holds a key");
    let (inner, executor, _) = registry();
    let watched = Arc::new(Watched {
        inner,
        keys: ops.device_keys.clone().expect("a scratch key file"),
        key_held_at_forget: Mutex::new(Vec::new()),
    });
    ops.proxy
        .bind_runs(&(Arc::clone(&watched) as Arc<dyn RunsPort>));

    let phone = RunScope::Device(DEVICE.to_owned());
    let script = executor.script("p1");
    watched.create(phone.clone(), "p1", body("m")).unwrap();
    watched.create(RunScope::Local, "l1", body("m")).unwrap();
    script.send(Cmd::Frame("f1".into())).unwrap();
    let mut reader = watched.events(&phone, "p1", 0).unwrap();
    assert!(next(&mut reader).await.is_some(), "the reader is open");
    // The script is dropped here; the run waits forever, as a live one does.
    (watched, reader)
}

/// The device's runs are gone, its reader ended, and this machine's run is
/// untouched; `forget_device` was called once, with no key in the file.
async fn assert_dropped_after_the_key(watched: &Watched, mut reader: RunEvents) {
    assert_eq!(
        *watched.key_held_at_forget.lock().unwrap(),
        [false],
        "the runs were dropped once, and only after the key was removed"
    );
    let phone = RunScope::Device(DEVICE.to_owned());
    assert!(
        watched.list(&phone).runs.is_empty(),
        "the device's runs are gone"
    );
    assert_eq!(next(&mut reader).await, None, "its reader has ended");
    let left: Vec<String> = watched
        .list(&RunScope::Local)
        .runs
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(left, ["l1"], "this machine's run is untouched");
}

#[tokio::test]
async fn forgetting_a_device_drops_its_runs_and_ends_their_readers_after_its_key_is_gone() {
    let (_, ops, _) = test_remote_ops().await;
    let (watched, reader) = a_device_with_a_run(&ops).await;

    assert!(ops.forget(DEVICE).await.expect("forget"));

    assert_dropped_after_the_key(&watched, reader).await;
}

/// A settings store that refuses every write.
struct Unwritable(Arc<dyn SettingsRepository>);

#[async_trait]
impl SettingsRepository for Unwritable {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        self.0.load().await
    }

    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Err(RepositoryError::Storage(
            "this store refuses writes".to_owned(),
        ))
    }
}

/// The roster write comes after the key is out of the file; when it fails,
/// `forget` fails, and the runs still go.
#[tokio::test]
async fn a_forget_whose_roster_write_fails_still_drops_the_runs_once_the_key_is_out() {
    let mut repos = CoreFactory::build_repos(setup_test_database().await.expect("in-memory DB"));
    gglib_core::services::AppCore::bare(repos.clone())
        .settings()
        .update(SettingsUpdate {
            remote_devices: Some(Some(vec![gglib_core::Device {
                id: DEVICE.to_owned(),
                label: None,
                joined_at: 1,
                redeemed_at: None,
                last_seen: None,
                peer: None,
                endpoint: None,
            }])),
            ..SettingsUpdate::default()
        })
        .await
        .expect("a roster row, so forget has a roster write to make");
    repos.settings = Arc::new(Unwritable(Arc::clone(&repos.settings)));
    let (core, proxy) = test_core_and_proxy_over(&repos);
    let emitter: Arc<dyn AppEventEmitter> = Arc::new(RecordingEmitter::default());
    let gateway = Arc::new(crate::RemoteGateway::new(Arc::clone(&emitter)));
    let ops = RemoteOps::new(proxy, core, gateway, emitter, Some(scratch_device_keys()));
    let (watched, reader) = a_device_with_a_run(&ops).await;

    let refused = ops.forget(DEVICE).await;

    assert!(refused.is_err(), "the roster write fails: {refused:?}");
    assert!(
        !read_keys(&ops)
            .expect("the key file reads")
            .contains_key(DEVICE),
        "the key was out of the file before the write that failed"
    );
    assert_dropped_after_the_key(&watched, reader).await;
}

/// A forget that fails before the key is out keeps the runs: the device may
/// still be admitted, and they are still its own.
#[cfg(unix)]
#[tokio::test]
async fn a_forget_that_fails_to_remove_the_key_keeps_the_runs() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().expect("a scratch directory");
    let keys_dir = dir.path().join("keys");
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let emitter: Arc<dyn AppEventEmitter> = Arc::new(RecordingEmitter::default());
    let gateway = Arc::new(crate::RemoteGateway::new(Arc::clone(&emitter)));
    let ops = RemoteOps::new(
        proxy,
        core,
        gateway,
        emitter,
        Some(keys_dir.join("remote_devices")),
    );
    let (watched, _reader) = a_device_with_a_run(&ops).await;
    let read_only = std::fs::Permissions::from_mode(0o500);
    std::fs::set_permissions(&keys_dir, read_only).expect("the key directory is read-only");

    let refused = ops.forget(DEVICE).await;
    std::fs::set_permissions(&keys_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    assert!(refused.is_err(), "the key write fails: {refused:?}");
    assert!(
        read_keys(&ops)
            .expect("the key file reads")
            .contains_key(DEVICE),
        "the key is still in the file"
    );
    assert!(
        watched.key_held_at_forget.lock().unwrap().is_empty(),
        "the runs were not dropped"
    );
    let phone = RunScope::Device(DEVICE.to_owned());
    assert_eq!(
        watched.list(&phone).runs.len(),
        1,
        "the device's run is kept"
    );
}
