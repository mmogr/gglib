//! A runtime whose admissions are scripted, for the tests about a server that
//! died or was freshly started.
//!
//! What the real runtime decides — which port a model answers on, and whether
//! this admission is the one that started it — a test here writes down, one
//! admission at a time. That is the only way to stage a dead upstream
//! followed by its restart, or a load that starts the server.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::{Client, Response};
use serde_json::json;
use tokio::net::TcpListener;

use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunningTarget,
};
use gglib_proxy::ServeConfig;
use gglib_proxy::slots::slot_bin_path;

use super::common::TaggedCatalog;
use super::spawn::{Spawned, defaults, spawn};

/// The one model [`spawn_cached`]'s catalog holds, with id 1.
pub(crate) const MODEL: &str = "test-model";

/// Where [`MODEL`] is admitted to: `port`, by an admission that started the
/// server there or one that found it running.
pub(crate) fn target(port: u16, just_started: bool) -> RunningTarget {
    RunningTarget::local(port, 1, MODEL.into(), 4096, just_started)
}

/// A runtime whose n-th admission is the n-th target of its script; the last
/// target repeats.
#[derive(Debug)]
pub(crate) struct ScriptedRuntime {
    script: Vec<RunningTarget>,
    admits: AtomicUsize,
}

impl ScriptedRuntime {
    pub(crate) fn new(script: Vec<RunningTarget>) -> Arc<Self> {
        assert!(!script.is_empty(), "a script needs one admission");
        Arc::new(Self {
            script,
            admits: AtomicUsize::new(0),
        })
    }

    /// How many admissions were asked for.
    pub(crate) fn admits(&self) -> usize {
        self.admits.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelRuntimePort for ScriptedRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        let n = self.admits.fetch_add(1, Ordering::SeqCst);
        let target = self.script[n.min(self.script.len() - 1)].clone();
        Ok(Admission::detached(target))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// A port bound and dropped at once: nothing is listening on it, which is
/// what an admission naming a server that has since died looks like.
pub(crate) async fn dead_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    listener.local_addr().unwrap().port()
}

/// A proxy over `runtime` whose catalog holds [`MODEL`], with the disk slot
/// cache on in `slot_dir`.
pub(crate) async fn spawn_cached(runtime: Arc<ScriptedRuntime>, slot_dir: &Path) -> Spawned {
    spawn(ServeConfig {
        runtime_port: runtime,
        catalog_port: Arc::new(TaggedCatalog {
            name: MODEL.into(),
            tags: vec![],
            dialect: None,
        }),
        cache_enabled: true,
        slot_dir: Some(slot_dir.to_path_buf()),
        ..defaults().await
    })
    .await
}

/// Send one chat completion for [`MODEL`] under `session`.
pub(crate) async fn chat(proxy: &str, session: &str, streaming: bool) -> Response {
    Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .header("X-Gglib-Session-Id", session)
        .json(&json!({
            "model": MODEL,
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": streaming
        }))
        .send()
        .await
        .expect("proxy request")
}

/// Write [`MODEL`]'s slot file for `session`, as a save leaves one, and say
/// where it is.
///
/// A restore with no file on disk stops before it calls the upstream, so a
/// test that counts restore calls needs one there to tell a restore that ran
/// from one that was skipped.
pub(crate) fn plant_slot_file(slot_dir: &Path, session: &str) -> PathBuf {
    let path = slot_bin_path(slot_dir, 1, session);
    std::fs::write(&path, b"fake kv state").expect("write the slot file");
    path
}

/// Wait until the wall clock has left the second it is in.
///
/// The slot cache counts a server's start in whole seconds, because that is
/// what it compares a slot file's mtime with, and it records a restart only
/// when it is later than the start it holds. So a test's restart has to land
/// in a later second than the proxy's own start, and than any slot file the
/// restart is meant to make stale. At most a second of real time.
pub(crate) async fn next_second() {
    let secs = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after the epoch")
            .as_secs()
    };
    let from = secs();
    while secs() == from {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Wait for `happened` to hold, for at most two seconds.
///
/// A streamed reply ends for the client before its save is sent upstream, so
/// a test that has just read one to the end waits for the save: for the
/// upstream to have counted it, and, where the test goes on to touch the slot
/// file, for the proxy to have renamed it into place.
pub(crate) async fn eventually(what: &str, happened: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while !happened() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
