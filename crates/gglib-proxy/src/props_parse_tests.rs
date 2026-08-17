//! Tests for [`super::parse_props`] and the two halves of a
//! [`PropsReading`](crate::template_caps_read::PropsReading).
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use gglib_core::domain::{Support, TemplateCapsState, reasoning_effort_support};
use reqwest::StatusCode;

/// Trimmed from a real `GET /props` on the pinned build, bare launch —
/// the same run that produced [`UPSTREAM_DEFAULTS`]. Pre-caps shape: no
/// `chat_template_caps` key, the "old build" fixture.
const REAL_PROPS: &str = r#"{
    "model_path": "/models/Llama-3.2-3B-Instruct-UD-Q6_K_XL.gguf",
    "default_generation_settings": {
        "n_ctx": 4096,
        "params": {
            "temperature": 0.800000011920929,
            "top_p": 0.949999988079071,
            "top_k": 40,
            "repeat_penalty": 1.0,
            "presence_penalty": 0.0,
            "min_p": 0.05000000074505806,
            "dry_multiplier": 0.0,
            "samplers": ["penalties","dry","top_n_sigma","top_k","typ_p","top_p","min_p","xtc","temperature"]
        }
    }
}"#;

/// The independence fixture: `chat_template_caps` present (the nine keys as
/// measured on `b1-10bf611`, 2026-08-17; `supports_reasoning_effort: true`
/// is config A's measured value) in a body carrying no
/// `default_generation_settings.params`. The old parse collapsed this to a
/// single `Unavailable`, discarding the caps.
const CAPS_WITHOUT_PARAMS: &str = r#"{
    "model_path": "/models/gpt-oss.gguf",
    "chat_template_caps": {
        "supports_tools": true,
        "supports_tool_calls": true,
        "supports_system_role": true,
        "supports_parallel_tool_calls": true,
        "supports_preserve_reasoning": true,
        "supports_reasoning_effort": true,
        "supports_string_content": true,
        "supports_typed_content": false,
        "supports_object_arguments": false
    }
}"#;

/// A build that publishes the key but reports nothing under it.
const EMPTY_CAPS: &str = r#"{"chat_template_caps": {}}"#;

fn real_params() -> SlotParams {
    match parse_props(StatusCode::OK, REAL_PROPS).params {
        PropsResult::Available(p) => p,
        other => panic!("real /props must parse: {other:?}"),
    }
}

#[test]
fn a_real_props_payload_parses() {
    let p = real_params();
    assert_eq!(p.temperature, Some(0.800_000_011_920_929));
    assert_eq!(p.top_k, Some(40.0));
    assert_eq!(p.samplers.unwrap().len(), 9);
}

#[test]
fn a_props_body_without_params_is_unavailable() {
    let r = parse_props(StatusCode::OK, r#"{"model_path": "/x.gguf"}"#);
    assert!(matches!(r.params, PropsResult::Unavailable(_)), "{r:?}");
}

#[test]
fn a_non_success_status_is_unavailable() {
    let r = parse_props(StatusCode::NOT_FOUND, "");
    assert!(
        matches!(r.params, PropsResult::Unavailable(ref m) if m.contains("404")),
        "{r:?}"
    );
    assert!(
        matches!(r.caps, TemplateCapsState::Unreadable { .. }),
        "an HTTP failure loses both halves to the same cause: {r:?}"
    );
}

#[test]
fn unparseable_json_is_unavailable_rather_than_a_panic() {
    let r = parse_props(StatusCode::OK, "not json at all");
    assert!(matches!(r.params, PropsResult::Unavailable(_)), "{r:?}");
    assert!(
        matches!(r.caps, TemplateCapsState::Unreadable { .. }),
        "{r:?}"
    );
}

// ── The two halves are independent (ADR 0007) ─────────────────────────────

/// **The defect this restructure fixes.** Caps present, params absent: the
/// caps half must survive rather than being discarded with the params
/// failure.
#[test]
fn caps_survive_a_body_whose_params_are_missing() {
    let r = parse_props(StatusCode::OK, CAPS_WITHOUT_PARAMS);

    assert!(matches!(r.params, PropsResult::Unavailable(_)), "{r:?}");
    let caps = r.caps.caps().expect("caps half must be read");
    assert_eq!(caps.supports_reasoning_effort, Some(true));
    assert_eq!(caps.supports_typed_content, Some(false));
}

/// A pre-caps build: params half reads fine, caps half is `Unreadable` —
/// which is not the same fact as "read as unsupported".
#[test]
fn an_old_build_without_caps_reads_params_but_not_caps() {
    let r = parse_props(StatusCode::OK, REAL_PROPS);

    assert!(matches!(r.params, PropsResult::Available(_)), "{r:?}");
    match r.caps {
        TemplateCapsState::Unreadable { ref reason } => {
            assert!(reason.contains("chat_template_caps"), "{reason}");
        }
        other => panic!("a missing key must read as Unreadable, got {other:?}"),
    }
}

/// An empty caps object parses to all-`None` — and resolves to *unknown*,
/// never to nine `false`s: five of the nine default `true` upstream, so a
/// serde default would manufacture negatives out of silence.
#[test]
fn an_empty_caps_object_reads_as_all_none_and_resolves_unknown() {
    let r = parse_props(StatusCode::OK, EMPTY_CAPS);

    let caps = r.caps.caps().expect("an empty object is still a report");
    assert_eq!(
        serde_json::to_value(caps)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .filter(|v| !v.is_null())
            .count(),
        0,
        "every field must stay None"
    );
    assert_eq!(
        reasoning_effort_support(&Some(caps.clone())),
        Support::Unknown,
        "an absent field licenses no conclusion in either direction"
    );
}
