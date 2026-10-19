//! A reply the proxy is still streaming tells the admission queue it is
//! moving, so a request waiting behind it for the same model is not given up
//! as stalled; a silent one does not, so the queue's deadline still runs.

use std::time::Duration;

use serde_json::json;

use super::forward_stall_fixtures::{
    IDLE, Then, at_once, done, finish, frame, progress, run_turn_holding, text, upstream,
};
use super::*;
use gglib_core::domain::DialectSpec;

/// Records when a request reports progress on its lease.
#[derive(Debug, Default)]
struct Reports(std::sync::Mutex<Vec<std::time::Instant>>);

impl Reports {
    fn times(&self) -> Vec<std::time::Instant> {
        self.0.lock().unwrap().clone()
    }
}

impl gglib_core::ports::AdmissionRelease for Reports {
    fn release(&self, _slot: usize) {}

    fn progress(&self, _slot: usize) {
        self.0.lock().unwrap().push(std::time::Instant::now());
    }
}

fn holding(reports: &Arc<Reports>) -> gglib_core::ports::AdmissionLease {
    gglib_core::ports::AdmissionLease::new(Arc::clone(reports) as _, 0)
}

/// A reply still streaming tells the queue it is moving, so a request waiting
/// behind it is not given up as stalled: once for a whole short reply, however
/// many tokens it has, prefill included.
#[tokio::test]
async fn a_streaming_reply_reports_progress_on_its_lease() {
    let reports = Arc::new(Reports::default());
    let chunks = vec![progress(), text("Hel"), text("lo"), finish(), done()];
    let bytes = upstream(at_once(chunks), Then::Close);
    let turn = run_turn_holding(bytes, None, holding(&reports)).await;

    assert!(turn.outcome.upstream_stalled.is_none());
    assert_eq!(reports.times().len(), 1);
}

/// A tool call written as text reports progress while its markup arrives,
/// though the normalizer sends the drain nothing until the call is whole: a
/// long call is the reply still moving.
#[tokio::test]
async fn a_tool_call_still_assembling_reports_progress_before_it_is_whole() {
    let reports = Arc::new(Reports::default());
    let markup = [
        "<tool_call>\n",
        "<function=read_file>\n",
        "<parameter=path>\n",
    ]
    .into_iter()
    .chain([
        "src/lib.rs\n",
        "</parameter>\n",
        "</function>\n",
        "</tool_call>",
    ]);
    let mut chunks: Vec<_> = markup
        .map(|m| (IDLE / 2, frame(&json!({ "content": m }), None)))
        .collect();
    chunks.extend([(Duration::ZERO, finish()), (Duration::ZERO, done())]);

    let started = std::time::Instant::now();
    let bytes = upstream(chunks, Then::Close);
    let turn = run_turn_holding(bytes, Some(DialectSpec::qwen_xml()), holding(&reports)).await;

    assert!(turn.took > IDLE * 3, "took {:?}", turn.took);
    let first = reports.times().first().copied().expect("it reported");
    assert!(
        first.duration_since(started) < IDLE * 2,
        "first report after {:?}, the call was whole after {:?}",
        first.duration_since(started),
        turn.took
    );
}

/// An upstream that never sends a byte reports nothing, so the queue's
/// deadline still runs against a wedged request.
#[tokio::test]
async fn a_silent_upstream_reports_no_progress() {
    let reports = Arc::new(Reports::default());
    let turn = run_turn_holding(upstream(vec![], Then::Silence), None, holding(&reports)).await;

    assert!(
        turn.outcome.upstream_stalled.is_some(),
        "the silence is a stall"
    );
    assert!(reports.times().is_empty());
}
