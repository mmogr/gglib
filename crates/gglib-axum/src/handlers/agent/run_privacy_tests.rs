//! No prompt, frame, reasoning, tool argument or result, and no loop error
//! reaches a log line on any end an agent run takes. The saved transcript
//! is the one place they go.

use std::io::Write;
use std::sync::{Arc, Mutex};

use gglib_core::ports::RunsPort as _;

use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, prepared, reply, settled, start, state,
};

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
async fn nothing_a_run_carries_reaches_a_log_line() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // As in the runs' own privacy test: a second registered dispatcher makes
    // callsites other threads hit first consult this one too.
    let capture = tracing::Dispatch::new(subscriber);
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let _default = tracing::dispatcher::set_default(&capture);
    tracing::callsite::rebuild_interest_cache();

    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (done, _) = prepared(finished_reply(), End::Finish);
    start(&state, "done", Some(id), done).await;
    settled(&state).await;
    let (failed, _) = prepared(reply(), End::Fail);
    start(&state, "failed", Some(id), failed).await;
    settled(&state).await;
    let (cancelled, _) = prepared(reply(), End::Hang);
    start(&state, "cancelled", Some(id), cancelled).await;
    state.runs.cancel(&LOCAL, "cancelled").unwrap();
    settled(&state).await;
    let (stopped, _) = prepared(reply(), End::Hang);
    start(&state, "stopped", Some(id), stopped).await;
    state.runs.shutdown();
    settled(&state).await;

    let log = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    assert!(
        log.contains("agent run started"),
        "the capture works: {log}"
    );
    assert!(log.contains("reply was saved"), "the capture works: {log}");
    for secret in [
        "PROMPT-SECRET",
        "REASON-SECRET",
        "ARGUMENT-SECRET",
        "RESULT-SECRET",
        "ANSWER-SECRET",
        "SIGNATURE-SECRET",
    ] {
        assert!(!log.contains(secret), "{secret} reached a log line:\n{log}");
    }
}
