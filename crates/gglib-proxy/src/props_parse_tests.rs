//! Tests for [`super::parse_props`] and [`super::PropsResult`].
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use reqwest::StatusCode;

/// Trimmed from a real `GET /props` on the pinned build, bare launch —
/// the same run that produced [`UPSTREAM_DEFAULTS`].
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

fn real_params() -> SlotParams {
    match parse_props(StatusCode::OK, REAL_PROPS) {
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
    assert!(matches!(r, PropsResult::Unavailable(_)), "{r:?}");
}

#[test]
fn a_non_success_status_is_unavailable() {
    let r = parse_props(StatusCode::NOT_FOUND, "");
    assert!(
        matches!(r, PropsResult::Unavailable(ref m) if m.contains("404")),
        "{r:?}"
    );
}

#[test]
fn unparseable_json_is_unavailable_rather_than_a_panic() {
    let r = parse_props(StatusCode::OK, "not json at all");
    assert!(matches!(r, PropsResult::Unavailable(_)), "{r:?}");
}
