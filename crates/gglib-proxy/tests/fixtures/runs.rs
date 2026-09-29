//! A stand-in for the daemon's runs, for the proxy's `/v1/runs` tests.
//!
//! Records the scope of every call, so a test can say who the proxy took the
//! caller for, and answers from a script: an error to refuse with, whether
//! `create` is a new run, and whether a run's events end or stay open.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::stream::{self, StreamExt as _};
use gglib_core::domain::runs::{RunInfo, RunKind, RunList, RunStatus};
use gglib_core::ports::{Created, RunEvent, RunEvents, RunScope, RunsError, RunsPort};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use serde_json::Value;

/// The stub.
#[derive(Debug, Default)]
pub(crate) struct FakeRuns {
    /// Every call, as `(what, scope)`.
    pub(crate) calls: Mutex<Vec<(&'static str, RunScope)>>,
    /// The `after` the last `events` call was given.
    pub(crate) after: Mutex<Option<u32>>,
    /// Refuse every call with this, when set.
    pub(crate) fail: Mutex<Option<RunsError>>,
    /// `create` answers with an existing run.
    pub(crate) existing: AtomicBool,
    /// `events` yields one frame and then stays open.
    pub(crate) endless: AtomicBool,
}

impl FakeRuns {
    fn note(&self, what: &'static str, scope: &RunScope) -> Result<(), RunsError> {
        self.calls.lock().unwrap().push((what, scope.clone()));
        self.fail.lock().unwrap().map_or(Ok(()), Err)
    }

    /// The scopes the proxy called with, in order.
    pub(crate) fn scopes(&self) -> Vec<RunScope> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(_, s)| s.clone())
            .collect()
    }
}

/// The run every call answers with.
pub(crate) fn info(id: &str) -> RunInfo {
    RunInfo {
        id: id.to_owned(),
        kind: RunKind::Chat,
        status: RunStatus::Completed,
        model: None,
        device: None,
        created_at_ms: 1,
        finished_at_ms: Some(2),
        last_seq: 2,
        error: None,
    }
}

impl RunsPort for FakeRuns {
    fn create(&self, scope: RunScope, id: &str, _body: Value) -> Result<Created, RunsError> {
        self.note("create", &scope)?;
        Ok(Created {
            info: info(id),
            created: !self.existing.load(Ordering::SeqCst),
        })
    }

    fn list(&self, scope: &RunScope) -> RunList {
        let _ = self.note("list", scope);
        RunList {
            runs: vec![info("r1")],
        }
    }

    fn get(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError> {
        self.note("get", scope)?;
        Ok(info(id))
    }

    fn events(&self, scope: &RunScope, id: &str, after: u32) -> Result<RunEvents, RunsError> {
        self.note("events", scope)?;
        *self.after.lock().unwrap() = Some(after);
        let first = RunEvent::Frame {
            seq: 1,
            data: "{\"a\":1}".into(),
        };
        if self.endless.load(Ordering::SeqCst) {
            return Ok(Box::pin(stream::iter([first]).chain(stream::pending())));
        }
        Ok(Box::pin(stream::iter([first, RunEvent::End(info(id))])))
    }

    fn cancel(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError> {
        self.note("cancel", scope)?;
        Ok(info(id))
    }

    fn forget_device(&self, _device: &str) -> usize {
        0
    }
}

/// The real proxy holding `runs`, demanding no key.
pub(crate) async fn serve(
    runs: Option<Arc<FakeRuns>>,
) -> (String, tokio_util::sync::CancellationToken) {
    serve_demanding(None, runs).await
}

/// [`serve`], demanding `key` as the bearer when there is one.
pub(crate) async fn serve_demanding(
    key: Option<&str>,
    runs: Option<Arc<FakeRuns>>,
) -> (String, tokio_util::sync::CancellationToken) {
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        key.map(str::to_owned),
        "127.0.0.1",
        vec![],
    )
    .with_runs(runs.map(|r| r as Arc<dyn RunsPort>));
    let (base, _, cancel) = super::access::spawn_proxy(access).await;
    (base, cancel)
}

/// A response's status and JSON body.
pub(crate) async fn json(response: reqwest::Response) -> (reqwest::StatusCode, Value) {
    let status = response.status();
    let body = response.text().await.unwrap();
    let value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"));
    (status, value)
}

/// The error code in a proxy error body.
pub(crate) fn code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or_default()
}

/// Each route with its method, as a client reaches it.
pub(crate) fn routes(base: &str) -> Vec<reqwest::RequestBuilder> {
    let client = reqwest::Client::new();
    vec![
        client
            .put(format!("{base}/v1/runs/r1"))
            .json(&serde_json::json!({})),
        client.get(format!("{base}/v1/runs")),
        client.get(format!("{base}/v1/runs/r1")),
        client.get(format!("{base}/v1/runs/r1/events")),
        client.post(format!("{base}/v1/runs/r1/cancel")),
    ]
}
