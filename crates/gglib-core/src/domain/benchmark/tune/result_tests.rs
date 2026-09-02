//! Tests for the result types in `result.rs`.
//!
//! Split from `result.rs` to keep that file inside the 300-LOC budget the
//! complexity ratchet enforces — the repo's `*_tests.rs` sibling pattern.

use super::*;
use crate::domain::benchmark::tune::task::TaskCategory;

/// `CandidateSource` is `#[serde(tag = "kind")]` (internally tagged),
/// which only supports newtype variants whose inner value serializes as
/// a JSON object/map. `FamilyPreset` must therefore stay a *struct*
/// variant (`{ family: String }`), never a bare `FamilyPreset(String)`
/// newtype — the latter fails at serialization time with "cannot
/// serialize tagged newtype variant ... containing a string".
#[test]
fn candidate_source_family_preset_round_trips() {
    let source = CandidateSource::FamilyPreset {
        family: "qwen-coding".to_string(),
    };
    let json = serde_json::to_string(&source).expect("serializes");
    let round_tripped: CandidateSource = serde_json::from_str(&json).expect("deserializes");
    assert!(matches!(
        round_tripped,
        CandidateSource::FamilyPreset { .. }
    ));
}

/// `UserGrid` is the only unit variant left since `GgufAuthorDefault` was
/// deleted, so this is a single case rather than the loop it used to be.
#[test]
fn candidate_source_unit_variants_round_trip() {
    let json = serde_json::to_string(&CandidateSource::UserGrid).expect("serializes");
    let round_tripped: CandidateSource = serde_json::from_str(&json).expect("deserializes");
    assert!(matches!(round_tripped, CandidateSource::UserGrid));
}

fn result_generating(generated: GeneratedOutput) -> TuneTaskResult {
    TuneTaskResult {
        task_id: "long_context_planted_values".to_owned(),
        category: TaskCategory::LongContext,
        passed: true,
        tool_match_score: 1.0,
        loop_detected: false,
        stagnation_detected: false,
        iterations: 2,
        latency_ms: 950_000,
        completion_tokens: Some(32_986),
        time_to_first_tool_call_ms: Some(941_000),
        detail: None,
        unmeasured: None,
        transport_retries: 0,
        generated,
    }
}

/// **The distinction this struct exists to preserve.** Two runs that agree
/// on every number the eval used to record — same tokens, same latency, same
/// score, same `passed` — and that call for opposite responses. One is a
/// model thinking at length, a question about the sampling recipe; the other
/// is a model failing to stop, a bug. Before this, both serialized
/// identically and the report could not tell a reader which it had seen.
#[test]
fn a_thinking_run_serializes_differently_from_a_repeating_one() {
    let thinking = result_generating(GeneratedOutput {
        reasoning_chars: 131_000,
        answer_chars: 400,
        llm_calls: 3,
        max_tool_calls_in_batch: 1,
        system_warnings: 0,
    });
    let repeating = result_generating(GeneratedOutput {
        reasoning_chars: 0,
        answer_chars: 131_400,
        llm_calls: 3,
        max_tool_calls_in_batch: 512,
        system_warnings: 4,
    });

    // Everything the eval recorded before this change agrees.
    assert_eq!(thinking.completion_tokens, repeating.completion_tokens);
    assert_eq!(thinking.latency_ms, repeating.latency_ms);
    assert_eq!(thinking.passed, repeating.passed);
    assert_eq!(thinking.iterations, repeating.iterations);

    let a = serde_json::to_value(&thinking).expect("serializes");
    let b = serde_json::to_value(&repeating).expect("serializes");
    assert_ne!(
        a, b,
        "two runs that need opposite responses must not be one record"
    );

    // And the distinction survives being written and read back, or a stored
    // report loses it the moment anyone opens the file.
    let restored: TuneTaskResult = serde_json::from_value(a).expect("round-trips");
    assert_eq!(restored.generated.reasoning_chars, 131_000);
    assert_eq!(restored.generated.max_tool_calls_in_batch, 1);
}

/// A report written before any of this existed must still parse, and must
/// read as "nothing recorded" rather than as a model that generated nothing.
#[test]
fn a_report_from_before_this_existed_still_parses() {
    let mut legacy =
        serde_json::to_value(result_generating(GeneratedOutput::default())).expect("serializes");
    legacy
        .as_object_mut()
        .expect("object")
        .remove("generated")
        .expect("field was present");

    let restored: TuneTaskResult = serde_json::from_value(legacy).expect("deserializes");
    assert_eq!(restored.generated.llm_calls, 0);
    assert_eq!(restored.completion_tokens, Some(32_986));
}
