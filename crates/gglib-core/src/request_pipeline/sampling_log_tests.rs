//! Tests for [`super`] — the one line that records a request's sampling.
//!
//! These capture the event itself rather than asserting on the
//! [`SamplingDecision`](super::SamplingDecision) the renderer is handed: the
//! line is the record, so the line is what they read.

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
/// searched so that another event's `from` can never be read off as this
/// line's.
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
/// per-model recipe that names one.
fn non_reading_model() -> ModelContext {
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

/// The template caps are read, never used to edit the request (ADR 0007
/// finding 7c): a level resolved for a template that does not read it is
/// still sent, and the line says so with its rung.
#[test]
fn a_level_the_template_does_not_read_is_still_sent_and_reported() {
    let mut value = body();
    let events = capture(|| {
        apply(
            &mut value,
            &non_reading_model(),
            &SamplingLayers::default(),
            None,
        )
        .expect("no budget, so nothing to reject");
    });

    assert_eq!(value["reasoning_effort"], "high", "{value}");
    assert_eq!(on_resolved_line(&events, "reasoning_effort"), "Some(High)");
    assert!(
        on_resolved_line(&events, "from").contains("reasoning_effort=model"),
        "{events:?}"
    );
}

/// The same on a model that *does* read the variable: the caps make no
/// difference to what is sent.
#[test]
fn a_level_the_template_reads_is_reported_with_its_rung() {
    let ctx = ModelContext {
        template_caps: Some(TemplateCaps {
            supports_reasoning_effort: Some(true),
            ..TemplateCaps::default()
        }),
        ..non_reading_model()
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

/// An ordinary request gets its whole table.
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
    ] {
        let _ = on_resolved_line(&events, field);
    }
}
