//! The proxy's figures in the frame, as the server sends them: the cache's
//! reuse percent, and the repairs and loop-guard trips totals.

use std::collections::BTreeMap;

use super::super::wire::{AdmissionSnapshot, CacheStatus, CacheUsage, DashboardSnapshot};
use super::render_frame;
use gglib_core::domain::figures::rate_words;

/// The proxy's figures from known traffic, as `gglib-proxy` serves them.
const FIGURES: &str = include_str!("../../../../../contracts/proxy/figures.json");

/// The recorded frame's figures, printed: each recent request's rate in the
/// words Rust says it in, the cache's percent and the trips total, read
/// back through this dashboard's own mirror of the wire.
#[test]
fn the_recorded_figures_print_as_the_proxy_says_them() {
    let figures: serde_json::Value = serde_json::from_str(FIGURES).unwrap();
    let out = frame(serde_json::json!({
        "active_connections": [],
        "slots_available": false,
        "total_requests": 6,
        "tool_repairs_succeeded": figures["tool_repairs_succeeded"],
        "loop_guard_trips_total": figures["loop_guard_trips_total"],
        "cache": { "disk_enabled": false, "ram_state": "llama_default", "usage": figures["cache_usage"] },
        "recent_requests": figures["recent_requests"],
    }));
    let recent: Vec<&str> = out
        .lines()
        .skip_while(|line| *line != "Recent requests")
        .skip(1)
        .take(5)
        .collect();
    assert!(
        recent[0].ends_with("phi-4                    33 tokens \u{b7} 8.3 t/s"),
        "a tie goes up: {out}"
    );
    assert!(
        recent[1].ends_with("phi-4                    100 tokens \u{b7} 10 t/s"),
        "ten is said whole: {out}"
    );
    assert!(
        recent[2].ends_with("Qwen3-8B"),
        "cut short, no figures: {out}"
    );
    assert!(
        recent[3].ends_with("Qwen3-8B                 1 token"),
        "{out}"
    );
    assert!(
        recent[4].ends_with("gemma-3-12b              1,200 tokens \u{b7} 8.5 t/s"),
        "{out}"
    );
    // Every recorded rate in the words the proxy recorded beside it.
    let said: Vec<serde_json::Value> = figures["recent_requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["tokens_per_second"].as_f64().map(rate_words).into())
        .collect();
    assert_eq!(serde_json::Value::from(said), figures["rate_words"]);
    assert!(
        out.contains("  Reused         6,600 of 10,000 prompt tokens (66%) in 2 requests\n"),
        "{out}"
    );
    assert!(
        out.contains("  Last request   2,400 of 4,000 tokens from cache (60%)\n"),
        "{out}"
    );
    assert!(out.ends_with("Loop-guard trips:      3\n"), "{out}");
}

/// A frame read from the proxy's own JSON, as the dashboard reads one.
fn frame(body: serde_json::Value) -> String {
    let snapshot: DashboardSnapshot = serde_json::from_value(body).expect("a proxy's frame");
    render_frame("http://127.0.0.1:9944", &snapshot, 100)
}

fn reused(usage: &serde_json::Value) -> String {
    let out = frame(serde_json::json!({
        "active_connections": [],
        "slots_available": false,
        "total_requests": 3,
        "cache": { "disk_enabled": false, "ram_state": "llama_default", "usage": usage },
    }));
    out.lines()
        .find(|line| line.trim_start().starts_with("Reused"))
        .expect("the reused row")
        .to_owned()
}

/// The percent is the server's, after the tokens; a proxy older than the
/// field prints none rather than one worked out here.
#[test]
fn the_reused_row_says_the_servers_percent() {
    let usage = serde_json::json!({
        "reporting_requests": 3, "prompt_tokens": 30_342, "cached_tokens": 29_450,
        "reuse_percent": 97, "summary": "Prompt cache 97% reused",
    });
    assert_eq!(
        reused(&usage),
        "  Reused         29,450 of 30,342 prompt tokens (97%) in 3 requests"
    );

    let older = serde_json::json!({
        "reporting_requests": 3, "prompt_tokens": 30_342, "cached_tokens": 29_450,
    });
    assert_eq!(
        reused(&older),
        "  Reused         29,450 of 30,342 prompt tokens in 3 requests"
    );
}

/// A full re-prefill is a measured 0%, said as one.
#[test]
fn a_measured_zero_says_zero_percent() {
    let usage = serde_json::json!({
        "reporting_requests": 1, "prompt_tokens": 5_000, "cached_tokens": 0, "reuse_percent": 0,
    });
    assert_eq!(
        reused(&usage),
        "  Reused         0 of 5,000 prompt tokens (0%) in 1 requests"
    );
}

/// The totals close the frame: the repairs, and the trips the server summed.
#[test]
fn the_frame_ends_with_the_repairs_and_trips_totals() {
    let out = frame(serde_json::json!({
        "active_connections": [],
        "slots_available": false,
        "total_requests": 1_204,
        "tool_repairs_succeeded": 2,
        "loop_guard_trips_total": 1_031,
    }));
    assert!(
        out.ends_with(
            "Total requests served: 1204\nTool calls repaired:   2\nLoop-guard trips:      1,031\n"
        ),
        "{out}"
    );
}

/// A proxy older than the repairs and trips totals prints neither, never a
/// zero it did not count.
#[test]
fn an_older_proxy_prints_no_repairs_or_trips_total() {
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: None,
        total_requests: 0,
        tool_repairs_succeeded: None,
        loop_guard_trips_total: None,
        cache: Some(CacheStatus {
            disk_enabled: false,
            disk_suppressed_for_model: false,
            ram_budget_mb: None,
            ram_state: "llama_default".to_owned(),
            warnings: vec![],
            usage: CacheUsage::default(),
        }),
        admission: AdmissionSnapshot::default(),
        per_model_defects: BTreeMap::new(),
        sampling_audit: None,
        recent_requests: vec![],
    };
    let out = render_frame("http://127.0.0.1:9944", &snapshot, 100);
    assert!(!out.contains("Loop-guard trips"), "{out}");
    assert!(!out.contains("Tool calls repaired"), "{out}");
    assert!(out.ends_with("Total requests served: 0\n"), "{out}");
}
