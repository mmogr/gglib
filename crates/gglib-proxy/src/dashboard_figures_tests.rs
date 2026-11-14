//! The dashboard's figures as the proxy serves them, recorded in
//! `contracts/proxy/figures.json` for the GUI and `gglib proxy dashboard`
//! to read back: recent requests with their writing rates, the cache's
//! reuse, and the loop-guard trips total.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::defects::{LoopGuardTrip, ModelDefectLedger};
use gglib_core::domain::figures::rate_words;
use serde_json::{Value, json};

use super::*;
use crate::generation::TurnGeneration;
use crate::metrics::ContextSnapshot;

fn request(model: &str) -> ContextSnapshot {
    ContextSnapshot {
        interventions: Vec::new(),
        model_name: model.to_owned(),
        payload_chars_before: 2_000,
        payload_chars_after: 2_000,
        messages_truncated: 0,
        was_clamped: false,
        dialect_residue: false,
        tool_repaired: false,
        completion_tokens: None,
        reasoning_chars: 0,
        tool_calls: 0,
        finish_reason: None,
        generation_ms: None,
        tokens_per_second: None,
        loop_guard_trip: None,
        loop_guard_ended: false,
        recorded_at_secs: 1_790_000_000,
        seq: 0,
    }
}

fn generated(tokens: u32, ms: Option<u64>) -> TurnGeneration {
    TurnGeneration {
        completion_tokens: Some(tokens),
        finish_reason: Some("stop".to_owned()),
        generation_ms: ms,
        ..TurnGeneration::default()
    }
}

/// A frame's figures from known traffic: five finished requests (one of a
/// single token, so with no rate, one at ten exactly and one at a tie of
/// 8.25), one cut short before its usage, a cache
/// that served 6,600 of 10,000 prompt tokens (the last request 2,400 of
/// 4,000), and three loop-guard trips over two models. Beside each request,
/// the words its rate is said in.
fn figures() -> Value {
    let ledger = Arc::new(ModelDefectLedger::new());
    let metrics = ContextMetricsStore::new().with_ledger(Arc::clone(&ledger));
    for (model, generation) in [
        ("Qwen3-8B", Some(generated(812, Some(23_600)))),
        ("gemma-3-12b", Some(generated(1_200, Some(141_800)))),
        ("Qwen3-8B", Some(generated(1, Some(0)))),
        ("Qwen3-8B", None),
        // Ten exactly, said whole, and a tie, which goes up.
        ("phi-4", Some(generated(100, Some(10_000)))),
        ("phi-4", Some(generated(33, Some(4_000)))),
    ] {
        let seq = metrics.record(request(model));
        if let Some(generation) = generation {
            metrics.flag_generation(seq, generation);
        }
    }
    ledger.record_loop_guard_trip("Qwen3-8B", LoopGuardTrip::Loop);
    ledger.record_loop_guard_trip("Qwen3-8B", LoopGuardTrip::Stagnation);
    ledger.record_loop_guard_trip("gemma-3-12b", LoopGuardTrip::Loop);

    let cache_metrics = CacheMetricsStore::new();
    cache_metrics.record(6_000, Some(4_200));
    cache_metrics.record(4_000, Some(2_400));
    let cache = CacheStatusCache::new();
    cache.set(CacheStatus::build(
        false,
        true,
        CacheRamHealth::LlamaDefault,
    ));

    let snapshot = DashboardSnapshot::build(
        &ActiveConnectionsRegistry::new(),
        &SlotsCache::new(),
        &metrics,
        &UpstreamHealth::new(),
        &cache,
        &cache_metrics,
        &LaunchNarrationCache::new(),
        None,
        &SamplingAuditStore::new(),
    );
    let rates: Vec<Option<String>> = snapshot
        .recent_requests
        .iter()
        .map(|r| r.tokens_per_second.map(rate_words))
        .collect();
    json!({
        "recent_requests": snapshot.recent_requests,
        "rate_words": rates,
        "cache_usage": snapshot.cache.expect("a cache status").usage,
        "tool_repairs_succeeded": snapshot.tool_repairs_succeeded,
        "loop_guard_trips_total": snapshot.loop_guard_trips_total,
    })
}

#[test]
fn the_served_figures_are_the_ones_the_traffic_makes() {
    let figures = figures();
    assert_eq!(
        figures["rate_words"],
        json!(["34 t/s", "8.5 t/s", null, null, "10 t/s", "8.3 t/s"])
    );
    assert_eq!(figures["cache_usage"]["reuse_percent"], 66);
    assert_eq!(figures["cache_usage"]["last_reuse_percent"], 60);
    assert_eq!(figures["cache_usage"]["summary"], "Prompt cache 66% reused");
    assert_eq!(figures["loop_guard_trips_total"], 3);
}

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/proxy/figures.json")
}

/// The GUI and the CLI read these figures back and must say the same of
/// them. `GGLIB_RECORD_CONTRACTS=1` rewrites the file after a deliberate change.
#[test]
fn the_figures_contract_is_current() {
    let want = serde_json::to_string_pretty(&figures()).unwrap() + "\n";
    let path = contract_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write figures.json");
    }
    let have = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        have, want,
        "contracts/proxy/figures.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1"
    );
}
