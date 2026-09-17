//! Tests for [`super`] — the context-metrics store.
//!
//! Beside the module rather than inside it, the way `loop_guard_tests.rs` sits
//! beside `loop_guard.rs`. Declared as `mod tests` through `#[path]`, so every
//! test keeps the `metrics::tests::` path it had when it lived inline.

use super::*;

fn make_snapshot(model: &str) -> ContextSnapshot {
    ContextSnapshot {
        model_name: model.to_string(),
        payload_chars_before: 1_000,
        payload_chars_after: 800,
        messages_truncated: 1,
        was_clamped: false,
        grammar_enforced: false,
        dialect_residue: false,
        tool_repaired: false,
        loop_guard_trip: None,
        recorded_at_secs: 0,
        seq: 0,
    }
}

// ── Basic record + retrieve ───────────────────────────────────────────────

/// The counters must be *readable*, not merely recorded.
///
/// Until this existed the ledger's only reader was the auto-tune
/// scheduler, which went with ADR 0006 — leaving every per-model counter
/// accumulating into memory that nothing could observe. A day of traffic
/// would have produced evidence nobody could look at.
#[test]
fn per_model_counts_are_readable_after_recording() {
    let ledger = std::sync::Arc::new(gglib_core::domain::defects::ModelDefectLedger::new());
    let store = ContextMetricsStore::new().with_ledger(std::sync::Arc::clone(&ledger));

    store.record(make_snapshot("qwen-27b"));
    let seq = store.recent(1)[0].seq;
    store.flag_stream_error(seq);
    store.flag_truncated_generation(seq);
    store.flag_empty_response(seq, true);

    let counts = store.defect_counts();
    let qwen = counts.get("qwen-27b").expect("the model appears by name");
    assert_eq!(qwen.stream_errors, 1);
    assert_eq!(qwen.truncated_generations, 1);
    assert_eq!(qwen.empty_responses, 1);
    assert_eq!(qwen.reasoning_only, 1, "nested inside the empty total");
}

#[test]
fn defect_counts_are_empty_without_a_ledger() {
    let store = ContextMetricsStore::new();
    store.record(make_snapshot("qwen-27b"));
    assert!(store.defect_counts().is_empty());
}

/// The snapshot is where the guard's two verdicts used to become one bit. A
/// stagnation trip has to reach the ledger as one, or ADR 0011's criterion is
/// asked of a number that cannot answer it.
#[test]
fn a_trip_reaches_the_ledger_under_the_detector_that_raised_it() {
    let ledger = std::sync::Arc::new(gglib_core::domain::defects::ModelDefectLedger::new());
    let store = ContextMetricsStore::new().with_ledger(std::sync::Arc::clone(&ledger));

    let mut stagnant = make_snapshot("qwen-27b");
    stagnant.loop_guard_trip = Some(LoopGuardTrip::Stagnation);
    store.record(stagnant);
    store.record(make_snapshot("qwen-27b"));

    let qwen = store.defect_counts()["qwen-27b"];
    assert_eq!(qwen.loop_guard_stagnations, 1, "the stagnation trip");
    assert_eq!(qwen.loop_guard_loops, 0, "nothing looped");
    assert_eq!(qwen.loop_guard_trips, 1, "the sum");
    assert_eq!(qwen.requests, 2, "the trip and the forwarded request");
}

/// `recent_requests` is a public route, so the detector's spelling there is a
/// contract: snake case like every other enum on it, and `null` for a request
/// the guard let through.
#[test]
fn a_snapshot_names_the_detector_in_snake_case_or_null() {
    let mut snapshot = make_snapshot("a");
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json["loop_guard_trip"].is_null(), "{json}");

    snapshot.loop_guard_trip = Some(LoopGuardTrip::Stagnation);
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["loop_guard_trip"], "stagnation", "{json}");

    snapshot.loop_guard_trip = Some(LoopGuardTrip::Loop);
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["loop_guard_trip"], "loop", "{json}");
}

#[test]
fn record_single_snapshot_and_retrieve() {
    let store = ContextMetricsStore::new();
    store.record(make_snapshot("qwen-3b"));

    let recent = store.recent(10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].model_name, "qwen-3b");
    assert_eq!(recent[0].messages_truncated, 1);
    assert_eq!(store.total_requests(), 1);
}

#[test]
fn recent_returns_at_most_n() {
    let store = ContextMetricsStore::new();
    for i in 0..10 {
        store.record(make_snapshot(&format!("model-{i}")));
    }
    let recent = store.recent(3);
    assert_eq!(recent.len(), 3);
    // Should be the last 3: model-7, model-8, model-9
    assert_eq!(recent[0].model_name, "model-7");
    assert_eq!(recent[2].model_name, "model-9");
}

#[test]
fn recent_returns_all_when_fewer_than_n() {
    let store = ContextMetricsStore::new();
    store.record(make_snapshot("a"));
    store.record(make_snapshot("b"));

    let recent = store.recent(100);
    assert_eq!(recent.len(), 2);
}

#[test]
fn empty_store_returns_empty_vec() {
    let store = ContextMetricsStore::new();
    assert!(store.recent(10).is_empty());
    assert_eq!(store.total_requests(), 0);
}

// ── Ring-buffer capacity ──────────────────────────────────────────────────

#[test]
fn ring_buffer_caps_at_max_snapshots() {
    let store = ContextMetricsStore::new();
    let insert_count = MAX_SNAPSHOTS + 5; // 55

    for i in 0..insert_count {
        store.record(make_snapshot(&format!("model-{i}")));
    }

    // total_requests must reflect all 55 inserts.
    assert_eq!(store.total_requests(), 55);

    // recent(MAX_SNAPSHOTS) must return exactly 50 entries (not 55).
    let recent = store.recent(MAX_SNAPSHOTS);
    assert_eq!(recent.len(), MAX_SNAPSHOTS);

    // The retained entries must be the LATEST 50 (indices 5..54).
    assert_eq!(recent[0].model_name, "model-5");
    assert_eq!(recent[MAX_SNAPSHOTS - 1].model_name, "model-54");
}

#[test]
fn ring_buffer_exactly_at_capacity_does_not_evict() {
    let store = ContextMetricsStore::new();
    for i in 0..MAX_SNAPSHOTS {
        store.record(make_snapshot(&format!("m-{i}")));
    }
    assert_eq!(store.recent(MAX_SNAPSHOTS).len(), MAX_SNAPSHOTS);
    assert_eq!(store.total_requests(), MAX_SNAPSHOTS as u64);
}

// ── Counter ───────────────────────────────────────────────────────────────

#[test]
fn total_requests_increments_on_every_record() {
    let store = ContextMetricsStore::new();
    assert_eq!(store.total_requests(), 0);
    store.record(make_snapshot("a"));
    assert_eq!(store.total_requests(), 1);
    store.record(make_snapshot("b"));
    assert_eq!(store.total_requests(), 2);
}

// ── Dialect residue back-patching ─────────────────────────────────────────

#[test]
fn flag_by_seq_sets_the_snapshot_flag_and_counts() {
    let store = ContextMetricsStore::new();
    let a = store.record(make_snapshot("a"));
    let _b = store.record(make_snapshot("b"));

    store.flag_dialect_residue(a);

    let recent = store.recent(10);
    assert!(
        recent[0].dialect_residue,
        "flag lands on the right snapshot"
    );
    assert!(!recent[1].dialect_residue);
    assert_eq!(store.dialect_residue_total(), 1);
}

#[test]
fn flag_after_eviction_still_counts_in_the_total() {
    let store = ContextMetricsStore::new();
    let first = store.record(make_snapshot("victim"));
    for i in 0..MAX_SNAPSHOTS {
        store.record(make_snapshot(&format!("m-{i}")));
    }
    // `first` is evicted by now; flagging must not panic and the
    // eviction-safe total must still increment.
    store.flag_dialect_residue(first);
    assert_eq!(store.dialect_residue_total(), 1);
    assert!(
        store
            .recent(MAX_SNAPSHOTS)
            .iter()
            .all(|s| !s.dialect_residue)
    );
}

#[test]
fn seq_is_not_serialized() {
    let store = ContextMetricsStore::new();
    store.record(make_snapshot("a"));
    let json = serde_json::to_string(&store.recent(1)[0]).unwrap();
    assert!(json.contains("dialect_residue"));
    assert!(!json.contains("\"seq\""));
}

/// Attempts and successes are tracked separately because the ratio is the
/// diagnostic: many attempts with few successes means `required` is not
/// fixing what this model gets wrong, which is a different problem from an
/// unconstrained `auto` path.
#[test]
fn repair_totals_count_attempts_and_successes_separately() {
    let store = ContextMetricsStore::new();
    let a = store.record(make_snapshot("m"));
    let b = store.record(make_snapshot("m"));

    store.flag_tool_repair(a, true);
    store.flag_tool_repair(b, false);

    assert_eq!(store.tool_repairs_attempted(), 2);
    assert_eq!(store.tool_repairs_succeeded(), 1);
}

/// The per-snapshot flag marks a repair that *worked*: a failed attempt
/// forwarded the original, so that turn's output was never repaired.
#[test]
fn only_a_successful_repair_flags_its_snapshot() {
    let store = ContextMetricsStore::new();
    let seq = store.record(make_snapshot("m"));

    store.flag_tool_repair(seq, false);
    assert!(!store.recent(10)[0].tool_repaired);

    let seq2 = store.record(make_snapshot("m"));
    store.flag_tool_repair(seq2, true);
    assert!(store.recent(10).iter().any(|s| s.tool_repaired));
}

/// Eviction must not lose the totals — the same contract
/// `dialect_residue_total` holds.
#[test]
fn repair_totals_survive_eviction() {
    let store = ContextMetricsStore::new();
    let seq = store.record(make_snapshot("m"));
    store.flag_tool_repair(seq, true);

    for _ in 0..MAX_SNAPSHOTS + 5 {
        store.record(make_snapshot("m"));
    }

    assert_eq!(store.tool_repairs_attempted(), 1);
    assert_eq!(store.tool_repairs_succeeded(), 1);
}
