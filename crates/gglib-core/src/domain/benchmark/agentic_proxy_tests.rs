//! Tests for the proxy arm's part of the report in `agentic_proxy.rs`.

use super::super::agentic_tests::{report_with, scored_result, scores};
use super::*;
use crate::domain::benchmark::agentic::{AgenticEvalConfig, EvalArm};

fn runs(id: &str, scores: &[f64]) -> Vec<TuneTaskResult> {
    scores.iter().map(|s| scored_result(id, *s)).collect()
}

fn settings() -> ProxyArmSettings {
    ProxyArmSettings {
        tool_call_repair: true,
        loop_guard_mode: LoopGuardMode::Note,
    }
}

/// Raw-auto is the baseline: a pair the proxy scored higher is a win for the
/// proxy, and the delta is `proxy − raw_auto`, the same orientation as
/// `gglib − raw`. Reversed, a proxy that repaired every call would read as a
/// regression.
#[test]
fn the_proxy_is_the_treatment_and_raw_auto_the_baseline() {
    let tasks = vec![
        ProxyTaskRuns {
            task_id: "a".into(),
            raw_auto: runs("a", &[0.5, 0.5]),
            proxy: runs("a", &[1.0, 1.0]),
        },
        ProxyTaskRuns {
            task_id: "b".into(),
            raw_auto: runs("b", &[1.0]),
            proxy: runs("b", &[1.0]),
        },
    ];
    let arms = ProxyArms::assemble(
        scores(0.6, None, 0.6),
        scores(0.9, None, 0.9),
        &ScoreWeights::default(),
        ModelDefectCounts::default(),
        settings(),
        tasks,
    );
    let paired = arms.paired.expect("three measured pairs");
    assert_eq!(paired.pairs, 3);
    assert_eq!((paired.wins, paired.losses, paired.ties), (2, 0, 1));
    assert!(paired.mean_delta > 0.0, "{paired:?}");
    let tool = arms.delta.tool_accuracy.expect("both arms measured it");
    assert!((tool - 0.3).abs() < 1e-9, "proxy − raw_auto, got {tool}");
}

/// A pair either side of which never reached the model is counted apart,
/// the rule the raw-versus-gglib pairing keeps.
#[test]
fn an_unmeasured_side_drops_its_pair() {
    let mut proxy = runs("a", &[1.0, 1.0]);
    proxy[1].unmeasured = Some("connection refused".into());
    let arms = ProxyArms::assemble(
        scores(0.5, None, 0.5),
        scores(1.0, None, 1.0),
        &ScoreWeights::default(),
        ModelDefectCounts::default(),
        settings(),
        vec![ProxyTaskRuns {
            task_id: "a".into(),
            raw_auto: runs("a", &[0.5, 0.5]),
            proxy,
        }],
    );
    let paired = arms.paired.expect("one measured pair");
    assert_eq!((paired.pairs, paired.unmeasured_pairs), (1, 1));
}

/// Reports are stored as JSON and read back, and every report written before
/// the proxy arm existed has no `proxy` key.
#[test]
fn a_report_without_the_proxy_arm_reads_back_as_none() {
    let mut json = serde_json::to_value(report_with(None, 0.7)).expect("serializes");
    json.as_object_mut().expect("object").remove("proxy");
    let report: AgenticEvalReport = serde_json::from_value(json).expect("a legacy row parses");
    assert!(report.proxy.is_none());
}

/// And a report that has one keeps it, counts included, through the same
/// round trip: the counts are the only record of whether repair engaged.
#[test]
fn a_report_with_the_proxy_arm_round_trips_its_counts() {
    let mut report = report_with(None, 0.7);
    let defects = ModelDefectCounts {
        requests: 12,
        repairs_attempted: 3,
        repairs_succeeded: 2,
        ..ModelDefectCounts::default()
    };
    report.proxy = Some(ProxyArms::assemble(
        scores(0.5, None, 0.5),
        scores(0.8, None, 0.8),
        &ScoreWeights::default(),
        defects,
        settings(),
        Vec::new(),
    ));
    let text = serde_json::to_string(&report).expect("serializes");
    let back: AgenticEvalReport = serde_json::from_str(&text).expect("parses");
    let arms = back.proxy.expect("kept");
    assert_eq!(arms.defects, defects);
    assert_eq!(arms.settings, settings());
}

/// The wire names the GUI's `EvalArm` union and the event stream carry.
#[test]
fn the_new_arms_have_snake_case_wire_names() {
    assert_eq!(serde_json::to_value(EvalArm::RawAuto).unwrap(), "raw_auto");
    assert_eq!(serde_json::to_value(EvalArm::Proxy).unwrap(), "proxy");
}

/// Off unless asked for, so an eval that does not ask costs what it did.
#[test]
fn the_proxy_arm_is_off_by_default() {
    let config: AgenticEvalConfig =
        serde_json::from_str(r#"{"model_id": 1, "task_suite": {"source": "default"}}"#)
            .expect("minimal body deserializes");
    assert!(!config.include_proxy);
}

/// A report stored before a counter existed still reads, with that counter
/// at zero: the counts are part of every stored report with the pair.
#[test]
fn a_stored_count_without_a_newer_counter_reads_it_as_zero() {
    let mut stored = serde_json::to_value(ModelDefectCounts {
        requests: 7,
        repairs_attempted: 2,
        ..ModelDefectCounts::default()
    })
    .expect("serializes");
    stored
        .as_object_mut()
        .expect("object")
        .remove("repairs_succeeded");
    let back: ModelDefectCounts = serde_json::from_value(stored).expect("still parses");
    assert_eq!(
        (
            back.requests,
            back.repairs_attempted,
            back.repairs_succeeded
        ),
        (7, 2, 0)
    );
}
