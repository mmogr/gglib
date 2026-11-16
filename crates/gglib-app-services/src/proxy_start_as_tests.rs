//! Tests for [`ProxyOps::start_as`]: a start that finds the endpoint on
//! leaves it alone unless told to replace it, and a replacement is one
//! operation that says `proxy_stopped` and `proxy_started` once each, and
//! puts back what ran when its own start is refused.

use std::sync::Arc;

use gglib_core::events::AppEvent;
use gglib_core::ports::PinnedSpec;
use gglib_db::{CoreFactory, setup_test_database};
use gglib_runtime::proxy::{ProxyConfig, ProxyStatus};

use crate::error::GuiError;
use crate::proxy_switch::{StartAnswer, StartOutcome};
use crate::test_support::{RecordingEmitter, test_core_and_proxy_heard};

struct Fixture {
    proxy: Arc<crate::ProxyOps>,
    heard: Arc<RecordingEmitter>,
}

impl Fixture {
    async fn new() -> Self {
        let pool = setup_test_database().await.expect("in-memory DB");
        let heard = Arc::new(RecordingEmitter::default());
        let (_core, proxy) = test_core_and_proxy_heard(
            &CoreFactory::build_repos(pool),
            Arc::clone(&heard) as Arc<dyn gglib_core::ports::AppEventEmitter>,
        );
        Self { proxy, heard }
    }

    /// What was said after the first `since` events, as `/api/events`
    /// sends each.
    fn said(&self, since: usize) -> Vec<String> {
        self.heard.events().iter().skip(since).map(frame).collect()
    }
}

fn frame(event: &AppEvent) -> String {
    serde_json::to_string(event).expect("an event serializes")
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port()
}

fn on(port: u16) -> ProxyConfig {
    ProxyConfig {
        host: "127.0.0.1".into(),
        port,
        ..ProxyConfig::default()
    }
}

fn qwen() -> PinnedSpec {
    PinnedSpec {
        id: 3,
        name: "Qwen3-4B".into(),
        ..PinnedSpec::default()
    }
}

/// Off, the endpoint is started; asked for again as it is, nothing
/// changes and nothing is said.
#[tokio::test]
async fn the_same_endpoint_asked_again_changes_nothing() {
    let fx = Fixture::new().await;
    let port = free_port();

    let first = fx.proxy.start_as(on(port), None, false).await.unwrap();
    let heard = fx.heard.events().len();
    let again = fx.proxy.start_as(on(port), None, false).await.unwrap();
    let said = fx.said(heard);
    fx.proxy.stop().await.unwrap();

    assert!(
        matches!(first, StartAnswer::On(a, StartOutcome::Started) if a.port() == port),
        "{first:?}"
    );
    assert!(
        matches!(again, StartAnswer::On(a, StartOutcome::Unchanged) if a.port() == port),
        "{again:?}"
    );
    assert!(said.is_empty(), "{said:?}");
}

/// Asked for otherwise without `replace`, the endpoint is left as it runs
/// and the answer says how the start differs.
#[tokio::test]
async fn a_different_start_without_replace_leaves_the_endpoint_alone() {
    let fx = Fixture::new().await;
    let port = free_port();
    fx.proxy.start_as(on(port), None, false).await.unwrap();
    let heard = fx.heard.events().len();

    let answer = fx
        .proxy
        .start_as(on(port), Some(qwen()), false)
        .await
        .unwrap();
    let said = fx.said(heard);
    let pinned = fx.proxy.pinned();
    fx.proxy.stop().await.unwrap();

    let StartAnswer::Differs(change) = answer else {
        panic!("a pinned start differs from every model: {answer:?}");
    };
    assert_eq!(
        change.now,
        format!("on at port {port}, serving every model")
    );
    assert_eq!(change.action, "serve only Qwen3-4B");
    assert!(pinned.is_none(), "nothing changed: {pinned:?}");
    assert!(said.is_empty(), "{said:?}");
}

/// With `replace`, the endpoint is switched in one operation: stopped and
/// started once each, said once each, and pinned as asked; switching back
/// to every model is the same.
#[tokio::test]
async fn a_replacement_says_stopped_then_started_once_each() {
    let fx = Fixture::new().await;
    let port = free_port();
    fx.proxy.start_as(on(port), None, false).await.unwrap();
    let heard = fx.heard.events().len();

    let answer = fx
        .proxy
        .start_as(on(port), Some(qwen()), true)
        .await
        .unwrap();
    let said = fx.said(heard);
    let pinned = fx.proxy.pinned().map(|p| p.name);

    assert!(
        matches!(answer, StartAnswer::On(a, StartOutcome::Replaced) if a.port() == port),
        "{answer:?}"
    );
    assert_eq!(
        said,
        [
            frame(&AppEvent::proxy_stopped()),
            frame(&AppEvent::proxy_started(port))
        ]
    );
    assert_eq!(pinned.as_deref(), Some("Qwen3-4B"));

    let back = fx.proxy.start_as(on(port), None, true).await.unwrap();
    let pinned = fx.proxy.pinned();
    fx.proxy.stop().await.unwrap();
    assert!(
        matches!(back, StartAnswer::On(_, StartOutcome::Replaced)),
        "{back:?}"
    );
    assert!(pinned.is_none(), "every model again: {pinned:?}");
}

/// A runtime that takes every pin but refuses the second: the endpoint's
/// first start takes one, and the one a refused switch puts back is refused.
#[derive(Debug, Default)]
struct RefusesThePinPutBack {
    pin: std::sync::Mutex<Option<PinnedSpec>>,
    pins_taken: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl gglib_core::ports::ModelRuntimePort for RefusesThePinPutBack {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: gglib_core::ports::LaunchOverrides,
    ) -> Result<gglib_core::ports::Admission, gglib_core::ports::ModelRuntimeError> {
        Err(gglib_core::ports::ModelRuntimeError::Internal(
            "this test runtime starts nothing".to_owned(),
        ))
    }

    async fn current_model(&self) -> Option<gglib_core::ports::RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), gglib_core::ports::ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(
        &self,
        _model_id: u32,
    ) -> Result<bool, gglib_core::ports::ModelRuntimeError> {
        Ok(false)
    }

    fn pinned(&self) -> Option<PinnedSpec> {
        self.pin.lock().unwrap().clone()
    }

    fn set_pin(&self, pin: Option<PinnedSpec>) -> Result<(), gglib_core::ports::ModelRuntimeError> {
        if pin.is_some()
            && self
                .pins_taken
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                == 1
        {
            return Err(gglib_core::ports::ModelRuntimeError::Internal(
                "the pin is refused".to_owned(),
            ));
        }
        *self.pin.lock().unwrap() = pin;
        Ok(())
    }
}

/// A switch whose new start is refused, and whose old endpoint cannot be
/// put back either, leaves the endpoint off, no pin held, and says so.
#[tokio::test]
async fn a_refused_switch_that_cannot_put_the_endpoint_back_says_it_is_off() {
    let pool = setup_test_database().await.expect("in-memory DB");
    let heard = Arc::new(RecordingEmitter::default());
    let (_core, proxy) = crate::test_support::test_core_and_proxy_telling(
        &CoreFactory::build_repos(pool),
        Arc::new(RefusesThePinPutBack::default()),
        Arc::clone(&heard) as Arc<dyn gglib_core::ports::AppEventEmitter>,
    );
    let port = free_port();
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let taken = holder.local_addr().unwrap().port();
    proxy
        .start_as(on(port), Some(qwen()), false)
        .await
        .expect("pinned to Qwen3-4B");

    let refused = proxy.start_as(on(taken), None, true).await;
    let status = proxy.status().await;
    let pinned = proxy.pinned();
    drop(holder);

    let Err(GuiError::Conflict(reason)) = refused else {
        panic!("the port asked for is taken: {refused:?}");
    };
    assert!(reason.contains(&taken.to_string()), "{reason}");
    assert!(
        reason.ends_with("The endpoint is off now: turn it on again."),
        "{reason}"
    );
    assert!(matches!(status, ProxyStatus::Stopped), "{status:?}");
    assert!(pinned.is_none(), "no pin outlives it: {pinned:?}");
}

/// A replacement whose start is refused, because another program holds the
/// port asked for, puts back what ran and says so.
#[tokio::test]
async fn a_refused_replacement_puts_the_endpoint_back_and_says_so() {
    let fx = Fixture::new().await;
    let port = free_port();
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let taken = holder.local_addr().unwrap().port();
    fx.proxy.start_as(on(port), None, false).await.unwrap();

    let refused = fx.proxy.start_as(on(taken), None, true).await;
    let status = fx.proxy.status().await;
    fx.proxy.stop().await.unwrap();
    drop(holder);

    let Err(GuiError::Conflict(reason)) = refused else {
        panic!("the port asked for is taken: {refused:?}");
    };
    assert!(reason.contains(&taken.to_string()), "{reason}");
    assert!(
        reason.ends_with(&format!(
            "The endpoint is back as it was, on at port {port}, serving every model."
        )),
        "{reason}"
    );
    assert!(
        matches!(status, ProxyStatus::Running { address } if address.port() == port),
        "{status:?}"
    );
}
