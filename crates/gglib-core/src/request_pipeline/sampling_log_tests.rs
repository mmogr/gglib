//! Tests for [`super`] — the one line that records a request's sampling.
//!
//! These capture the event itself rather than asserting on the
//! [`SamplingDecision`](super::SamplingDecision) the renderer is handed. The
//! decision is post-gate no matter where the line is rendered from; what these
//! pin is that the *line* is, which is the whole reason the renderer was moved
//! out of stage 4. A test that read the struct back would pass just as happily
//! with the `debug!` restored to its old position.

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::subscriber::with_default;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::Registry;

use crate::domain::{DefaultsOrigin, InferenceConfig, ReasoningEffort, TemplateCaps};
use crate::request_pipeline::{ModelContext, SamplingLayers, apply};

/// Every `(field, rendered value)` pair of every event, in order.
type Captured = Arc<Mutex<Vec<(String, String)>>>;

struct Recorder(Captured);

impl Visit for Recorder {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((field.name().to_owned(), format!("{value:?}")));
    }

    /// `from = %…` and the `"sampling resolved"` message itself arrive as
    /// strings; without this they would render with their quotes on and every
    /// `contains` below would be testing the `Debug` impl rather than the line.
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((field.name().to_owned(), value.to_owned()));
    }
}

struct CaptureLayer(Captured);

impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        event.record(&mut Recorder(Arc::clone(&self.0)));
    }
}

/// Run `f` with every event captured.
///
/// Reads the sink through the shared handle rather than taking it apart.
/// `Arc::try_unwrap` was used here, on the assumption that `with_default`
/// returning means the subscriber — and so the layer's clone of this `Arc` —
/// has been dropped. That is not something `tracing` promises, and it is not
/// true under concurrency: `tracing-core` registers every `Dispatch` for
/// callsite-interest purposes, and another thread hitting a callsite for the
/// first time can hold a strong reference to ours while we are asking.
///
/// Measured in a standalone reproduction of this exact shape — 32 threads,
/// distinct callsites, 51,200 rounds — the sink was still shared on return
/// 1,288 times, 2.5%. Every one of those would have been a panic here, which
/// is what made this module fail about once in six full-workspace runs while
/// passing hundreds of times in isolation. The same run measured the
/// replacement: reading through the shared handle lost an event 0 times, and
/// returned the full set on all 1,288 of the rounds that would have panicked.
///
/// Nothing needed exclusive ownership: `f` has finished and the layer records
/// synchronously on this thread, so the events are all in by now and a clone
/// of the vector is the whole requirement.
fn capture(f: impl FnOnce()) -> Vec<(String, String)> {
    let sink: Captured = Arc::default();
    let subscriber = Registry::default().with(CaptureLayer(Arc::clone(&sink)));
    with_default(subscriber, f);
    sink.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// The fields of the one event whose `message` is `name`.
///
/// `tracing` records `message` first and the declared fields after it, so an
/// event is the slice from its message up to the next one. Sliced rather than
/// searched because stage 5b's own `debug!` also carries a `from` — taking the
/// first match in the whole capture would read one line's provenance off
/// another's.
fn fields_of<'a>(events: &'a [(String, String)], name: &str) -> &'a [(String, String)] {
    let start = events
        .iter()
        .position(|(k, v)| k == "message" && v.contains(name))
        .unwrap_or_else(|| panic!("no '{name}' event in {events:?}"));
    let rest = &events[start + 1..];
    let end = rest
        .iter()
        .position(|(k, _)| k == "message")
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The value of `field` on the `"sampling resolved"` event.
fn on_resolved_line(events: &[(String, String)], field: &str) -> String {
    fields_of(events, "sampling resolved")
        .iter()
        .find(|(k, _)| k == field)
        .map_or_else(
            || panic!("no {field} on the resolved line in {events:?}"),
            |(_, v)| v.clone(),
        )
}

/// A model whose template positively does not read `reasoning_effort`, with a
/// per-model recipe that names one — the shape stage 5b exists for.
fn suppressing_model() -> ModelContext {
    ModelContext {
        catalog_resolved: true,
        template_caps: Some(TemplateCaps {
            supports_reasoning_effort: Some(false),
            ..TemplateCaps::default()
        }),
        inference_defaults: Some(InferenceConfig {
            reasoning_effort: Some(ReasoningEffort::High),
            ..InferenceConfig::default()
        }),
        defaults_origin: Some(DefaultsOrigin::User),
        ..ModelContext::passthrough()
    }
}

fn body() -> serde_json::Value {
    serde_json::json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]})
}

/// **The defect this module was created to fix.** Rendered inside stage 4, the
/// line reported `Some(High)` from the model rung for a level stage 5b then
/// deleted — and nothing downstream echoes either reasoning control, so that
/// line was the only account of the request there would ever be.
#[test]
fn a_suppressed_level_is_not_reported_as_resolved() {
    let events = capture(|| {
        let mut value = body();
        apply(
            &mut value,
            &suppressing_model(),
            &SamplingLayers::default(),
            None,
        )
        .expect("no budget, so nothing to reject");
    });

    assert_eq!(
        on_resolved_line(&events, "reasoning_effort"),
        "None",
        "the line must describe what was sent, not what stage 4 folded"
    );
    assert!(
        on_resolved_line(&events, "from").contains("reasoning_effort=suppressed-by-template"),
        "provenance must name the suppression: {events:?}"
    );
}

/// The other half: on a model that *does* read the variable, the same line has
/// to carry the level and the rung. A renderer that simply stopped printing
/// reasoning would pass the test above.
#[test]
fn a_surviving_level_is_still_reported_with_its_rung() {
    let ctx = ModelContext {
        template_caps: Some(TemplateCaps {
            supports_reasoning_effort: Some(true),
            ..TemplateCaps::default()
        }),
        ..suppressing_model()
    };
    let events = capture(|| {
        let mut value = body();
        apply(&mut value, &ctx, &SamplingLayers::default(), None).expect("applies");
    });

    assert_eq!(on_resolved_line(&events, "reasoning_effort"), "Some(High)");
    assert!(
        on_resolved_line(&events, "from").contains("reasoning_effort=model"),
        "{events:?}"
    );
}

/// Stage 5b's line is not made redundant by the move: after suppression the
/// resolved line reads `None`, so the level that was dropped and the rung that
/// asked for it exist nowhere else.
#[test]
fn the_gate_still_names_what_it_dropped() {
    let events = capture(|| {
        let mut value = body();
        apply(
            &mut value,
            &suppressing_model(),
            &SamplingLayers::default(),
            None,
        )
        .expect("applies");
    });

    let fields = fields_of(&events, "reasoning_effort suppressed");
    assert!(
        fields.iter().any(|(k, v)| k == "level" && v == "high"),
        "{events:?}"
    );
    assert!(
        fields.iter().any(|(k, v)| k == "from" && v == "model"),
        "{events:?}"
    );
}

/// An ordinary request must still get its whole table — the move must not have
/// dropped a field on the way out of stage 4.
#[test]
fn every_resolved_parameter_still_reaches_the_line() {
    let events = capture(|| {
        let mut value = body();
        apply(
            &mut value,
            &ModelContext::passthrough(),
            &SamplingLayers::default(),
            None,
        )
        .expect("applies");
    });

    for field in [
        "temperature",
        "top_p",
        "top_k",
        "max_tokens",
        "presence_penalty",
        "repeat_penalty",
        "min_p",
        "frequency_penalty",
        "dynatemp_range",
        "dynatemp_exponent",
        "top_n_sigma",
        "dry_multiplier",
        "dry_base",
        "dry_allowed_length",
        "dry_penalty_last_n",
        "reasoning_effort",
        "reasoning_budget_tokens",
        "from",
        "floor",
        "agentic_turn",
    ] {
        let _ = on_resolved_line(&events, field);
    }
}
