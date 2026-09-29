//! `forget` drops the device's runs, and only once no key admits it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gglib_core::access::load_device_keys;
use gglib_core::domain::runs::{RunInfo, RunList};
use gglib_core::ports::{Created, RunEvents, RunScope, RunsError, RunsPort};
use serde_json::Value;

use super::super::device_keys::write_keys;
use crate::runs::RunRegistry;
use crate::runs::test_executor::{Cmd, body, next, registry};
use crate::test_support_remote::test_remote_ops;

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

#[tokio::test]
async fn forgetting_a_device_drops_its_runs_and_ends_their_readers_after_its_key_is_gone() {
    let (_, ops, _) = test_remote_ops().await;
    write_keys(
        &ops,
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

    assert!(ops.forget(DEVICE).await.expect("forget"));

    assert_eq!(
        *watched.key_held_at_forget.lock().unwrap(),
        [false],
        "the runs were dropped once, and only after the key was removed"
    );
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
