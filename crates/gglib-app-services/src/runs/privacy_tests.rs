//! A frame's content and a request body never reach a log line, at any
//! level, on any path a run takes: a reply, an error frame, a refusal, a
//! cancel, a forgotten device and shutdown.

use std::io::Write;
use std::sync::{Arc, Mutex};

use gglib_core::ports::{RunScope, RunsPort};
use serde_json::json;

use super::RunRegistry;
use super::chat::ChatExecutor;
use super::fake_proxy::{FakeProxy, STREAM_HEAD, door};
use super::test_executor::{HandClock, drain};

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn no_frame_or_request_body_reaches_a_log_line() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // Other tests in this process hit the same callsites on other threads.
    // With one dispatcher registered, tracing-core decides a new callsite's
    // interest from the default of whichever thread hits it first, which is
    // none there, and caches "never". A second registered dispatcher makes
    // it consult every registered one instead, the capture included.
    let capture = tracing::Dispatch::new(subscriber);
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let _default = tracing::dispatcher::set_default(&capture);
    tracing::callsite::rebuild_interest_cache();

    let mut proxy = FakeProxy::start().await;
    let runs = RunRegistry::new(
        Arc::new(ChatExecutor::new(door(proxy.addr, Some("KEY-SECRET")))),
        HandClock::at(1_000).clock(),
    );
    let body = json!({ "model": "MODEL-SECRET", "messages": [{ "role": "user", "content": "PROMPT-SECRET" }] });
    runs.create(RunScope::Local, "r1", body.clone()).unwrap();
    (&mut proxy.seen).await.unwrap();
    proxy.say(STREAM_HEAD);
    proxy.say("data: {\"choices\":[{\"delta\":{\"content\":\"FRAME-SECRET\"}}]}\n\n");
    proxy.say("data: {\"error\":{\"message\":\"ERROR-SECRET\",\"code\":\"upstream_timeout\"}}\n\n");
    drain(runs.events(&RunScope::Local, "r1", 0).unwrap()).await;

    let phone = RunScope::Device("phone".into());
    runs.create(phone.clone(), "p1", body.clone()).unwrap();
    runs.cancel(&phone, "p1").unwrap();
    runs.create(phone, "p2", body.clone()).unwrap();
    runs.forget_device("phone");
    runs.create(RunScope::Local, "r2", body).unwrap();
    runs.shutdown();
    tokio::task::yield_now().await;

    let log = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    assert!(log.contains("run ended"), "the capture works: {log}");
    for secret in [
        "MODEL-SECRET",
        "PROMPT-SECRET",
        "FRAME-SECRET",
        "ERROR-SECRET",
        "KEY-SECRET",
    ] {
        assert!(!log.contains(secret), "{secret} reached a log line:\n{log}");
    }
}
