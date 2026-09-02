//! Tests for [`super::agentic`].
//!
//! Several pin figures from the 2026-08-28 run that motivated this
//! module's corrections, so a regression shows up as that run reading
//! wrongly again rather than as an abstract assertion.

use super::*;

/// The sibling of the `TuneConfig` assertion in `tune::config` — see there
/// for why an absent `weights` key and a `null` one are not interchangeable
/// on the wire. `gglib benchmark agentic` has no weight flags at all, so
/// every config the CLI builds carries `weights: None`; if `null` were the
/// spelling serde produced, every agentic run would be the broken case.
/// (The server re-serializes this same type into `benchmark_runs`, where
/// `weights` is `Some` for any client that sent one.)
#[test]
fn a_config_with_no_weights_omits_the_key_rather_than_nulling_it() {
    let config: AgenticEvalConfig =
        serde_json::from_str(r#"{"model_id": 1, "task_suite": {"source": "default"}}"#)
            .expect("minimal body deserializes");
    assert!(config.weights.is_none());

    let body = serde_json::to_value(&config).expect("serializes");
    assert!(
        !body.as_object().expect("object").contains_key("weights"),
        "serialized body must not carry a `weights` key: {body}"
    );
}

fn scores(tool_accuracy: f64, loop_avoidance: Option<f64>, composite: f64) -> ArmScores {
    ArmScores {
        tool_accuracy,
        loop_avoidance,
        loop_eligible: usize::from(loop_avoidance.is_some()),
        task_completion: 0.25,
        composite,
        tg_tps: Some(30.0),
        total_completion_tokens: Some(1_000),
        total_wall_ms: 1_000,
        measured_wall_ms: 1_000,
        mean_time_to_first_tool_call_ms: Some(100.0),
        median_time_to_first_tool_call_ms: Some(100.0),
        generated: GeneratedOutput::default(),
        seeds: 3,
        runs: 12,
        unmeasured_runs: 0,
        transport_retries: 0,
    }
}

fn task_result(id: &str, passed: bool) -> TuneTaskResult {
    TuneTaskResult {
        task_id: id.to_owned(),
        category: TaskCategory::SingleCall,
        passed,
        tool_match_score: if passed { 1.0 } else { 0.0 },
        loop_detected: false,
        stagnation_detected: false,
        iterations: 1,
        latency_ms: 10,
        completion_tokens: Some(100),
        time_to_first_tool_call_ms: Some(5),
        detail: None,
        unmeasured: None,
        transport_retries: 0,
        generated: GeneratedOutput::default(),
    }
}

fn comparison(raw: &[bool], gglib: &[bool]) -> AgenticTaskComparison {
    AgenticTaskComparison {
        task_id: "t".to_owned(),
        category: TaskCategory::SingleCall,
        raw: raw.iter().map(|p| task_result("t", *p)).collect(),
        gglib: gglib.iter().map(|p| task_result("t", *p)).collect(),
    }
}

/// An arm whose axes agree with its composite: every axis sits at `value`,
/// so any weighting of them returns `value` too.
///
/// Needed because the compared composite is derived from the axes rather
/// than read off the stored field. A fixture that sets the two
/// independently is describing an arm that could not exist, and would make
/// these tests agree with an implementation that ignored its own inputs.
fn uniform_arm(value: f64) -> ArmScores {
    let mut arm = scores(value, None, value);
    arm.task_completion = value;
    arm
}

fn report_with(control: Option<ArmScores>, gglib_composite: f64) -> AgenticEvalReport {
    AgenticEvalReport {
        model_name: "m".to_owned(),
        quantization: None,
        param_count_b: 1.0,
        ctx_size: 4096,
        raw: uniform_arm(0.5),
        gglib: uniform_arm(gglib_composite),
        delta: AgenticEvalReport::delta_of(
            &uniform_arm(0.5),
            &uniform_arm(gglib_composite),
            &ScoreWeights::default(),
        ),
        tasks: vec![],
        seeds: DEFAULT_SEEDS.to_vec(),
        control,
        raw_replicate: None,
        replicate_seeds: vec![],
        raw_replicates: vec![],
        replicate_seed_sets: vec![],
        paired: None,
    }
}

/// A report whose raw and A/A arms differ by `noise` and whose gglib arm
/// sits `effect` above raw, with everything else held fixed.
fn report_with_replicate(effect: f64, noise: f64) -> AgenticEvalReport {
    let raw = uniform_arm(0.500);
    let gglib = uniform_arm(0.500 + effect);
    let replicate = uniform_arm(0.500 + noise);
    AgenticEvalReport {
        delta: AgenticEvalReport::delta_of(&raw, &gglib, &ScoreWeights::default()),
        raw,
        gglib,
        raw_replicate: Some(replicate),
        replicate_seeds: replicate_seeds(&DEFAULT_SEEDS),
        ..report_with(None, 0.9)
    }
}

// =========================================================================
// Seeds
// =========================================================================

/// The default has to be more than one, or the eval is back to reporting a
/// single draw from the model's output distribution as a measurement.
#[test]
fn the_default_seed_set_is_larger_than_one() {
    assert!(DEFAULT_SEEDS.len() > 1);
    assert_eq!(default_seeds(), DEFAULT_SEEDS.to_vec());
}

/// A config written before seeds existed must still deserialize, and must
/// pick up the multi-seed default rather than silently staying single.
#[test]
fn a_legacy_config_gains_the_default_seeds_and_control() {
    let json = r#"{"model_id": 1, "task_suite": {"source": "default"}}"#;
    let config: AgenticEvalConfig = serde_json::from_str(json).expect("deserializes");

    assert_eq!(config.seeds, DEFAULT_SEEDS.to_vec());
    assert!(config.include_control);
    assert!(config.replicate_raw, "the A/A arm is on by default");
    assert_eq!(
        config.control_seeds, 1,
        "the control does not pay for precision it is never read for"
    );
}

/// An explicitly empty seed list is a real choice — one unseeded run — and
/// must survive as empty rather than being backfilled with the default.
#[test]
fn an_explicitly_empty_seed_list_stays_empty() {
    let json = r#"{"model_id": 1, "task_suite": {"source": "default"}, "seeds": []}"#;
    let config: AgenticEvalConfig = serde_json::from_str(json).expect("deserializes");

    assert!(config.seeds.is_empty());
}

/// A legacy stored report has no sample size recorded, and `1` is what it
/// actually was — not zero, which would render as "no runs".
#[test]
fn a_legacy_report_reads_as_a_single_seed() {
    let json = r#"{
        "tool_accuracy": 0.5, "task_completion": 0.5, "composite": 0.5
    }"#;
    let scores: ArmScores = serde_json::from_str(json).expect("deserializes");

    assert_eq!(scores.seeds, 1);
}

// =========================================================================
// Per-task stability
// =========================================================================

/// The finding a multi-seed eval exists to surface: a task that passes
/// under one arm on every seed and under the other on only some.
#[test]
fn pass_counts_report_each_arms_seeds_separately() {
    let cmp = comparison(&[true, false, false], &[true, true, true]);
    assert_eq!(cmp.pass_counts(), (1, 3));
}

/// A task that disagrees with itself across seeds is where suite variance
/// comes from, and it must be findable whichever arm was unstable.
#[test]
fn a_task_that_flips_between_seeds_is_unstable() {
    assert!(comparison(&[true, false], &[true, true]).is_unstable());
    assert!(comparison(&[true, true], &[false, true]).is_unstable());
}

/// Consistent outcomes are stable even when the two arms disagree with
/// each other — that is a *result*, not instability.
#[test]
fn arms_disagreeing_consistently_is_not_instability() {
    assert!(!comparison(&[false, false, false], &[true, true, true]).is_unstable());
    assert!(!comparison(&[true, true], &[true, true]).is_unstable());
}

// =========================================================================
// The positive control
// =========================================================================

/// The control's job: a known-bad sampling change must register, or the
/// run proved nothing about its own sensitivity.
#[test]
fn a_control_that_scored_well_below_gglib_demonstrates_sensitivity() {
    let verdict = report_with(Some(scores(0.2, None, 0.30)), 0.90).control_verdict();
    assert!(matches!(verdict, Some(ControlVerdict::Moved { .. })));
}

/// **The failure this exists to catch.** Forcing temperature 2.0 barely
/// changed the score, so the apparatus cannot detect a sampling change —
/// and therefore cannot support any other delta in the report.
#[test]
fn a_control_that_barely_moved_reports_failure() {
    let verdict = report_with(Some(scores(0.88, None, 0.89)), 0.90).control_verdict();
    assert!(matches!(verdict, Some(ControlVerdict::TooSmall { .. })));
}

/// **The failure that was measured, and that the old bool hid.** A control
/// scoring *above* the gglib arm contradicts its premise rather than
/// failing a threshold, and it must not be reported as "barely moved".
#[test]
fn a_control_that_scored_higher_is_its_own_verdict() {
    let report = report_with(Some(scores(0.95, None, 0.99)), 0.90);
    let verdict = report.control_verdict().expect("a verdict");
    assert!(!verdict.demonstrated_sensitivity());

    match verdict {
        ControlVerdict::WrongDirection { gap } => {
            assert!((gap - 0.09).abs() < 1e-9, "gap is reported positive: {gap}");
        }
        other => panic!("expected WrongDirection, got {other:?}"),
    }
}

/// The two failures are distinct states: one says the suite is too small
/// or the effect too subtle, the other says the control itself is broken.
/// They want different fixes, so they must not render the same.
#[test]
fn a_small_gap_and_a_wrong_direction_are_different_verdicts() {
    let small = report_with(Some(scores(0.5, None, 0.88)), 0.90);
    let wrong = report_with(Some(scores(0.5, None, 0.99)), 0.90);

    assert!(matches!(
        small.control_verdict(),
        Some(ControlVerdict::TooSmall { .. })
    ));
    assert!(matches!(
        wrong.control_verdict(),
        Some(ControlVerdict::WrongDirection { .. })
    ));
    assert_ne!(small.control_verdict(), wrong.control_verdict());
}

/// Both failure gaps are reported as positive magnitudes, so neither
/// renders with a sign that has to be interpreted.
#[test]
fn every_verdict_reports_a_positive_gap() {
    for report in [
        report_with(Some(scores(0.2, None, 0.30)), 0.90),
        report_with(Some(scores(0.5, None, 0.88)), 0.90),
        report_with(Some(scores(0.5, None, 0.99)), 0.90),
    ] {
        let gap = match report.control_verdict().expect("a verdict") {
            ControlVerdict::Moved { gap }
            | ControlVerdict::TooSmall { gap }
            | ControlVerdict::WrongDirection { gap } => gap,
        };
        assert!(gap >= 0.0, "{gap}");
    }
}

/// **The control must disable truncation, not only raise the temperature.**
/// llama.cpp runs the truncation samplers first, so a `top_k` left in force
/// absorbs the temperature — measured on Qwen3.5-4B, where a
/// temperature-only control scored *above* both real arms.
#[test]
fn the_control_disables_every_truncation_sampler() {
    let (temperature, top_k, top_p, min_p) = control_sampling();

    assert!((temperature - CONTROL_TEMPERATURE).abs() < f32::EPSILON);
    assert_eq!(top_k, 0, "top_k must be disabled, not merely widened");
    assert!(
        (top_p - 1.0).abs() < f32::EPSILON,
        "top_p keeps the nucleus"
    );
    assert!(min_p.abs() < f32::EPSILON, "min_p cuts no tail");
}

/// Not run is not the same as ran-and-failed — the same distinction the
/// sampling readback draws between blind and zero divergences.
#[test]
fn no_control_arm_claims_nothing_either_way() {
    assert!(report_with(None, 0.90).control_verdict().is_none());
}

// =========================================================================
// Runs that measured nothing
// =========================================================================

/// **The state that must never render as a score.** Every run failed
/// before reaching the model, so the composite is arithmetic over 45 zeros
/// and describes nothing.
#[test]
fn an_arm_where_no_run_reached_the_model_is_an_empty_column() {
    let arm = ArmScores {
        runs: 45,
        unmeasured_runs: 45,
        ..scores(0.244, None, 0.222)
    };

    assert!(arm.is_empty_column());
    assert!(
        !arm.is_partly_unmeasured(),
        "wholly empty is its own state, not a severe partial"
    );
}

/// A partial is a different state wanting a different action: the arm has
/// real observations, diluted by a knowable number of empty ones.
#[test]
fn an_arm_with_some_failures_is_partly_unmeasured() {
    let arm = ArmScores {
        runs: 45,
        unmeasured_runs: 7,
        ..scores(0.5, None, 0.5)
    };

    assert!(arm.is_partly_unmeasured());
    assert!(!arm.is_empty_column());
}

/// The ordinary case must trip neither check, or the warning becomes noise
/// and stops being read.
#[test]
fn a_fully_measured_arm_is_neither() {
    let arm = ArmScores {
        runs: 45,
        unmeasured_runs: 0,
        ..scores(0.9, None, 0.9)
    };

    assert!(!arm.is_empty_column());
    assert!(!arm.is_partly_unmeasured());
}

/// An arm that ran nothing at all has no runs to be empty *of*, and must
/// not be reported as a failure of the upstream.
#[test]
fn an_arm_with_no_runs_is_not_an_empty_column() {
    let arm = ArmScores {
        runs: 0,
        unmeasured_runs: 0,
        ..scores(0.0, None, 0.0)
    };

    assert!(!arm.is_empty_column());
}

/// A stored report from before the field existed must read as "every run
/// was measured", which is what it meant.
#[test]
fn a_legacy_arm_reads_as_fully_measured() {
    let json = r#"{
        "tool_accuracy": 0.5, "task_completion": 0.5, "composite": 0.5, "runs": 9
    }"#;
    let arm: ArmScores = serde_json::from_str(json).expect("deserializes");

    assert_eq!(arm.unmeasured_runs, 0);
    assert!(!arm.is_empty_column());
}

// =========================================================================
// The A/A arm
// =========================================================================

/// The design of the arm in one assertion: replaying the same seeds would
/// measure decode determinism, not the seed-draw variance that actually
/// limits the primary comparison.
#[test]
fn the_replicate_seeds_are_disjoint_from_the_primary_ones() {
    let replicate = replicate_seeds(&DEFAULT_SEEDS);

    assert_eq!(replicate.len(), DEFAULT_SEEDS.len());
    for seed in &DEFAULT_SEEDS {
        assert!(
            !replicate.contains(seed),
            "seed {seed} was reused, so the A/A arm would measure nothing"
        );
    }
}

/// Derived, not drawn: a noise floor that changed every run could not be
/// compared against the run before it.
#[test]
fn the_replicate_seeds_are_reproducible() {
    assert_eq!(
        replicate_seeds(&DEFAULT_SEEDS),
        replicate_seeds(&DEFAULT_SEEDS)
    );
    const { assert!(REPLICATE_SEED_OFFSET != 0) };
}

/// The noise floor is a distance, so which arm scored higher is irrelevant
/// to it — an A/A arm that came out *ahead* of raw is drift just the same.
#[test]
fn the_noise_floor_is_a_distance_not_a_direction() {
    let above = report_with_replicate(0.20, 0.05);
    let below = report_with_replicate(0.20, -0.05);

    assert!((above.noise_floor().unwrap() - 0.05).abs() < 1e-9);
    assert!((below.noise_floor().unwrap() - 0.05).abs() < 1e-9);
}

/// **What the arm exists for.** An effect the same size as the eval's own
/// drift must not be reported as a finding.
#[test]
fn an_effect_no_bigger_than_the_drift_is_within_noise() {
    let report = report_with_replicate(0.04, 0.03);

    let verdict = report.effect_verdict().expect("the A/A arm ran");
    assert!(!verdict.exceeds_noise());
    assert!((verdict.ratio().unwrap() - 4.0 / 3.0).abs() < 1e-9);
}

/// And an effect several times the drift must clear it, or the arm would
/// veto every result it was added to qualify.
#[test]
fn an_effect_well_past_the_drift_clears_it() {
    let report = report_with_replicate(0.30, 0.03);

    let verdict = report.effect_verdict().expect("the A/A arm ran");
    assert!(verdict.exceeds_noise());
    assert!((verdict.ratio().unwrap() - 10.0).abs() < 1e-9);
}

/// A negative effect that clears the drift is still a resolved measurement.
/// Reporting only favourable findings as real is the failure mode an A/A
/// arm is supposed to prevent, not introduce.
#[test]
fn a_negative_effect_can_also_exceed_the_noise() {
    let verdict = report_with_replicate(-0.30, 0.03)
        .effect_verdict()
        .expect("the A/A arm ran");

    assert!(verdict.exceeds_noise());
    assert!(verdict.effect() < 0.0, "the sign survives the verdict");
}

/// Two arms landing on the identical composite is an unresolved drift, not
/// an infinitely precise one, so nothing may divide by it.
#[test]
fn a_zero_drift_yields_no_ratio() {
    let report = report_with_replicate(0.08, 0.0);
    let verdict = report.effect_verdict().expect("the A/A arm ran");

    assert_eq!(verdict.ratio(), None);
    assert!(
        verdict.exceeds_noise(),
        "a real effect over no measured drift"
    );
}

/// Both terms zero is the vacuous case: no effect, no drift, and nothing
/// that may be reported as having exceeded anything.
#[test]
fn no_effect_over_no_drift_is_not_a_finding() {
    let report = report_with_replicate(0.0, 0.0);

    assert!(!report.effect_verdict().expect("ran").exceeds_noise());
}

/// Without the arm there is no basis for the judgement, and the report
/// must decline to make it rather than assume a floor of zero.
#[test]
fn no_replicate_arm_yields_no_effect_verdict() {
    let report = report_with(None, 0.90);

    assert_eq!(report.noise_floor(), None);
    assert!(report.effect_verdict().is_none());
}

/// The threshold has to be above 1.0: an effect merely *equal* to the
/// drift is exactly the case the arm was added to catch.
#[test]
fn the_noise_ratio_demands_more_than_parity() {
    const { assert!(EFFECT_NOISE_RATIO > 1.0) };
    assert!(
        !report_with_replicate(0.05, 0.05)
            .effect_verdict()
            .expect("ran")
            .exceeds_noise()
    );
}

/// A stored report from before the A/A arm existed must read as "no
/// replicate ran" rather than failing to deserialize.
#[test]
fn a_legacy_report_has_no_replicate_arm() {
    let json = r#"{
        "model_name": "m", "quantization": null, "param_count_b": 1.0,
        "ctx_size": 4096,
        "raw": {"tool_accuracy": 0.5, "task_completion": 0.5, "composite": 0.5},
        "gglib": {"tool_accuracy": 0.9, "task_completion": 0.9, "composite": 0.9},
        "delta": {"tool_accuracy": 0.4, "task_completion": 0.4, "composite": 0.4},
        "tasks": []
    }"#;
    let report: AgenticEvalReport = serde_json::from_str(json).expect("deserializes");

    assert!(report.raw_replicate.is_none());
    assert!(report.replicate_seeds.is_empty());
    assert!(report.effect_verdict().is_none());
}

/// The threshold has to be a real gap, not any difference at all, or noise
/// would satisfy the control.
#[test]
fn the_control_threshold_is_larger_than_rounding() {
    const { assert!(CONTROL_MIN_COMPOSITE_GAP > 0.0) };
    let exactly_at = report_with(
        Some(scores(0.5, None, 0.90 - CONTROL_MIN_COMPOSITE_GAP)),
        0.90,
    );
    let verdict = exactly_at.control_verdict();
    assert!(
        matches!(verdict, Some(ControlVerdict::Moved { .. })),
        "the bound is inclusive"
    );
}

/// The control must be unambiguously bad rather than marginally worse, or
/// "no difference" stays ambiguous between a broken harness and a robust
/// model.
#[test]
fn the_control_temperature_is_far_outside_any_sane_recipe() {
    const { assert!(CONTROL_TEMPERATURE >= 2.0) };
}

#[test]
fn delta_is_gglib_minus_raw() {
    let mut raw = scores(0.5, Some(0.75), 0.5);
    raw.task_completion = 0.25;
    let mut gglib = scores(0.9, Some(1.0), 0.9);
    gglib.task_completion = 0.75;

    let weights = ScoreWeights::default();
    let delta = AgenticEvalReport::delta_of(&raw, &gglib, &weights);
    assert!((delta.tool_accuracy.unwrap() - 0.4).abs() < 1e-9);
    assert!((delta.loop_avoidance.unwrap() - 0.25).abs() < 1e-9);
    assert!((delta.task_completion.unwrap() - 0.5).abs() < 1e-9);

    // Both arms measured all three axes, so the compared composite is the
    // ordinary weighted mean of each.
    let expected =
        weights.composite_of(0.9, Some(1.0), 0.75) - weights.composite_of(0.5, Some(0.75), 0.25);
    assert!((delta.composite.unwrap() - expected).abs() < 1e-9);
    assert!(delta.withheld.is_none());
}

/// **The axis asymmetry.** Each arm's stored composite is renormalized over
/// the axes *it* measured, so an arm with no loop-eligible run divides by
/// 0.6 where an arm with one divides by 0.9. Subtracting those two directly
/// compares the renormalization, and it does so in a consistent direction:
/// the arm that measured the extra axis is handed a free score on ground
/// its opponent was never scored on.
///
/// Here both arms are identical on every axis they share, so the only
/// honest difference is zero.
#[test]
fn an_axis_only_one_arm_measured_cannot_move_the_composite() {
    let weights = ScoreWeights::default();

    // Identical tool accuracy and task completion. Raw additionally scored
    // a perfect loop-avoidance; gglib never became loop-eligible.
    let mut raw = scores(0.9, Some(1.0), 0.0);
    raw.task_completion = 0.9;
    raw.composite = weights.composite_of(0.9, Some(1.0), 0.9);
    let mut gglib = scores(0.9, None, 0.0);
    gglib.task_completion = 0.9;
    gglib.composite = weights.composite_of(0.9, None, 0.9);

    let delta = AgenticEvalReport::delta_of(&raw, &gglib, &weights);
    assert!(
        (delta.composite.unwrap()).abs() < 1e-9,
        "two arms equal on every shared axis must not differ; got {:?} \
         (raw {:.4} vs gglib {:.4} as stored)",
        delta.composite,
        raw.composite,
        gglib.composite,
    );
    assert!(
        delta.loop_avoidance.is_none(),
        "an axis one arm never measured yields no difference"
    );
}

/// Runs that never reached the model dilute every mean on their arm, so the
/// arm-level differences are withheld rather than reported diluted — and
/// the effect verdict, which is built on the composite, goes with them.
#[test]
fn a_contaminated_arm_withholds_its_deltas_and_its_verdict() {
    let raw = uniform_arm(0.5);
    let mut gglib = uniform_arm(0.9);
    gglib.unmeasured_runs = 5;

    let delta = AgenticEvalReport::delta_of(&raw, &gglib, &ScoreWeights::default());
    assert_eq!(
        delta.withheld,
        Some(DeltaWithheld::ContaminatedByUnmeasuredRuns { raw: 0, gglib: 5 })
    );
    assert!(delta.composite.is_none());
    assert!(delta.tool_accuracy.is_none());
    assert!(delta.task_completion.is_none());

    let report = AgenticEvalReport {
        raw,
        gglib,
        delta,
        raw_replicate: Some(uniform_arm(0.51)),
        replicate_seeds: replicate_seeds(&DEFAULT_SEEDS),
        ..report_with(None, 0.9)
    };
    assert!(
        report.effect_verdict().is_none(),
        "a drift ratio taken on a diluted effect is a confident number about \
         two things that are not the same"
    );
}

/// A withheld delta must be distinguishable **on the wire**, not only in
/// the renderer that happens to look for it.
///
/// This is `a_blind_sampling_audit_serializes_differently_from_a_clean_one`
/// applied one layer up: the distinction dies at whichever layer collapses
/// it, and a `0.0` where a `null` belongs is exactly the collapse — it
/// reads as "these arms are identical" to every consumer downstream.
#[test]
fn a_withheld_delta_serializes_differently_from_a_measured_one() {
    let raw = uniform_arm(0.5);
    let clean = AgenticEvalReport::delta_of(&raw, &raw, &ScoreWeights::default());

    let mut contaminated_arm = uniform_arm(0.5);
    contaminated_arm.unmeasured_runs = 5;
    let withheld = AgenticEvalReport::delta_of(&raw, &contaminated_arm, &ScoreWeights::default());

    let clean_json = serde_json::to_value(&clean).expect("serializes");
    let withheld_json = serde_json::to_value(&withheld).expect("serializes");

    assert_ne!(clean_json, withheld_json);
    assert_eq!(clean_json["composite"], serde_json::json!(0.0));
    assert!(
        withheld_json["composite"].is_null(),
        "a withheld composite must not serialize as a measured zero: {withheld_json}"
    );
    assert!(withheld_json["withheld"].is_object());

    // And it must survive the round trip, or a stored report loses the
    // distinction the moment it is read back.
    let restored: ArmDelta = serde_json::from_value(withheld_json).expect("round-trips");
    assert!(restored.composite.is_none());
    assert!(restored.withheld.is_some());
}

/// The efficiency rows are ratios, not differences: lower is better on
/// both, so a subtraction would invert the struct's "positive means gglib
/// did better" convention. These are the figures from the run that
/// motivated the fix.
#[test]
fn efficiency_factors_are_raw_over_gglib() {
    let mut raw = scores(0.722, Some(1.0), 0.802);
    raw.total_wall_ms = 1_104_543;
    raw.measured_wall_ms = 1_104_543;
    raw.total_completion_tokens = Some(226_768);
    let mut gglib = scores(0.722, Some(1.0), 0.802);
    gglib.total_wall_ms = 4_806;
    gglib.measured_wall_ms = 4_806;
    gglib.total_completion_tokens = Some(49);

    let delta = AgenticEvalReport::delta_of(&raw, &gglib, &ScoreWeights::default());
    assert!((delta.wall_time_speedup.unwrap() - 229.83).abs() < 0.01);
    assert!((delta.completion_token_ratio.unwrap() - 4_627.92).abs() < 0.01);
}

/// The 2026-08-28 run, recomputed. Its efficiency table reported gglib at
/// `0.2×` wall time and `1.48×` tokens; both were artefacts of comparing
/// two arms over different sets of runs.
///
/// Figures are that run's: raw 683,265 ms over 63 measured runs and 34,475
/// tokens; gglib 3,553,427 ms over 63 runs of which 5 were five separate
/// ten-minute timeouts, leaving 553,409 ms over 58 measured runs and 23,256
/// tokens.
#[test]
fn the_run_that_motivated_this_reports_gglib_faster_not_five_times_slower() {
    let mut raw = scores(0.929, None, 0.0);
    raw.runs = 63;
    raw.unmeasured_runs = 0;
    raw.total_wall_ms = 683_265;
    raw.measured_wall_ms = 683_265;
    raw.total_completion_tokens = Some(34_475);

    let mut gglib = scores(0.966, None, 0.0);
    gglib.runs = 63;
    gglib.unmeasured_runs = 5;
    gglib.total_wall_ms = 3_553_427;
    gglib.measured_wall_ms = 553_409;
    gglib.total_completion_tokens = Some(23_256);

    let delta = AgenticEvalReport::delta_of(&raw, &gglib, &ScoreWeights::default());

    // Per measured run: raw 10,845 ms against gglib 9,541 ms.
    let speedup = delta.wall_time_speedup.expect("both arms measured runs");
    assert!(
        (speedup - 1.137).abs() < 0.01,
        "expected gglib ~1.14x faster per run, got {speedup:.3}x"
    );
    assert!(
        speedup > 1.0,
        "the arm that finished its measured work sooner must not read as slower"
    );

    // Per measured run: raw 547.2 tokens against gglib 401.0.
    let tokens = delta.completion_token_ratio.expect("both arms generated");
    assert!(
        (tokens - 1.365).abs() < 0.01,
        "expected 1.36x on per-run tokens, got {tokens:.3}x"
    );

    // And the quality axes stay withheld: five of the runs behind them
    // never reached the model.
    assert!(delta.composite.is_none());
    assert_eq!(
        delta.withheld,
        Some(DeltaWithheld::ContaminatedByUnmeasuredRuns { raw: 0, gglib: 5 })
    );
}

/// An infinite speedup is not a measurement.
#[test]
fn a_zero_denominator_yields_no_factor() {
    let raw = scores(0.5, Some(1.0), 0.5);
    let mut gglib = scores(0.5, Some(1.0), 0.5);
    gglib.total_wall_ms = 0;
    gglib.measured_wall_ms = 0;
    gglib.total_completion_tokens = Some(0);

    let delta = AgenticEvalReport::delta_of(&raw, &gglib, &ScoreWeights::default());
    assert!(delta.wall_time_speedup.is_none());
    assert!(delta.completion_token_ratio.is_none());
}

/// Subtracting against an arm that never risked a loop would be arithmetic
/// on a figure nobody observed — exactly the comparison that reported a
/// bare llama-server arm as beating the pipeline.
#[test]
fn an_unmeasured_arm_yields_no_loop_avoidance_delta() {
    let raw = scores(0.5, None, 0.5);
    let gglib = scores(0.9, Some(0.0), 0.9);
    assert!(
        AgenticEvalReport::delta_of(&raw, &gglib, &ScoreWeights::default())
            .loop_avoidance
            .is_none()
    );
    assert!(
        AgenticEvalReport::delta_of(&gglib, &raw, &ScoreWeights::default())
            .loop_avoidance
            .is_none()
    );
}

#[test]
fn config_round_trips_with_defaults() {
    let json = r#"{"model_id": 3, "task_suite": {"source": "default"}}"#;
    let config: AgenticEvalConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.model_id, 3);
    assert!(config.ctx_size.is_none());
}

// ── PairedEffect ─────────────────────────────────────────────────────────

/// A result with a chosen match score, for paired fixtures.
fn scored_result(id: &str, score: f64) -> TuneTaskResult {
    TuneTaskResult {
        tool_match_score: score,
        passed: score >= 1.0,
        ..task_result(id, false)
    }
}

fn paired_task(id: &str, raw_scores: &[f64], gglib_scores: &[f64]) -> AgenticTaskComparison {
    AgenticTaskComparison {
        task_id: id.to_owned(),
        category: TaskCategory::SingleCall,
        raw: raw_scores.iter().map(|s| scored_result(id, *s)).collect(),
        gglib: gglib_scores.iter().map(|s| scored_result(id, *s)).collect(),
    }
}

#[test]
fn paired_effect_counts_wins_losses_ties_and_means_the_deltas() {
    let tasks = vec![
        paired_task("a", &[0.5, 1.0], &[1.0, 1.0]), // one win, one tie
        paired_task("b", &[1.0], &[0.5]),           // one loss
    ];
    let paired = PairedEffect::from_tasks(&tasks).expect("pairs exist");
    assert_eq!(paired.pairs, 3);
    assert_eq!((paired.wins, paired.losses, paired.ties), (1, 1, 1));
    assert!((paired.mean_delta - 0.0).abs() < 1e-12, "{paired:?}");
    assert_eq!(paired.unmeasured_pairs, 0);
    // Two non-tied pairs is far below the Wilcoxon minimum.
    assert_eq!(paired.p_value, None);
}

/// A pair either side of which never reached the model is dropped and
/// counted, never scored — the same rule `ArmScores::unmeasured_runs`
/// applies arm-wide, kept at pair granularity here.
#[test]
fn paired_effect_drops_unmeasured_pairs_and_says_so() {
    let mut task = paired_task("a", &[0.5, 0.5], &[1.0, 1.0]);
    task.raw[1].unmeasured = Some("upstream unreachable".to_owned());
    let paired = PairedEffect::from_tasks(&[task]).expect("one live pair");
    assert_eq!(paired.pairs, 1);
    assert_eq!(paired.unmeasured_pairs, 1);
    assert_eq!(paired.wins, 1);
}

#[test]
fn paired_effect_is_none_when_nothing_was_measured() {
    assert!(PairedEffect::from_tasks(&[]).is_none());
    let mut task = paired_task("a", &[0.5], &[1.0]);
    task.gglib[0].unmeasured = Some("dead".to_owned());
    assert!(PairedEffect::from_tasks(&[task]).is_none());
}

/// Ten distinct all-positive deltas: W⁻ = 0, and the normal approximation
/// with continuity correction gives z = (0 − 27.5 + 0.5)/√96.25 ≈ −2.752,
/// p ≈ 0.0030. Pinned inside a band an implementation error of one rank,
/// one correction term, or a dropped tail would leave.
#[test]
fn wilcoxon_all_positive_deltas_is_a_strong_result() {
    let gglib: Vec<f64> = (1..=10).map(|i| f64::from(i) * 0.05).collect();
    let raw = vec![0.0; 10];
    let tasks = vec![paired_task("a", &raw, &gglib)];
    let p = PairedEffect::from_tasks(&tasks)
        .unwrap()
        .p_value
        .expect("ten non-tied pairs");
    assert!(p > 0.001 && p < 0.005, "p = {p}");
}

/// Symmetric wins and losses of matching magnitude: W⁻ lands on its null
/// mean and the one-sided p sits at chance.
#[test]
fn wilcoxon_balanced_deltas_read_as_chance() {
    let raw = vec![0.5; 10];
    let gglib = vec![0.6, 0.4, 0.7, 0.3, 0.8, 0.2, 0.9, 0.1, 1.0, 0.0];
    let tasks = vec![paired_task("a", &raw, &gglib)];
    let p = PairedEffect::from_tasks(&tasks)
        .unwrap()
        .p_value
        .expect("ten non-tied pairs");
    assert!(p > 0.4 && p < 0.6, "p = {p}");
}

/// Below the minimum the statistic says nothing — ties do not count
/// toward the minimum, because zeros are dropped before ranking.
#[test]
fn wilcoxon_says_nothing_below_the_minimum() {
    let raw = vec![0.5; 10];
    let mut gglib = vec![0.5; 10]; // ties everywhere...
    for (i, value) in gglib.iter_mut().enumerate().take(7) {
        *value = 0.01f64.mul_add(f64::from(u8::try_from(i).unwrap()), 0.6);
    }
    let tasks = vec![paired_task("a", &raw, &gglib)];
    let paired = PairedEffect::from_tasks(&tasks).unwrap();
    assert_eq!(paired.pairs, 10);
    assert_eq!(paired.wins, 7);
    assert_eq!(paired.p_value, None);
}

// ── Multi-pair A/A drift ────────────────────────────────────────────────

/// Three runs of the identical configuration give three pairwise gaps,
/// and the floor is their mean — more degrees of freedom, same estimator.
#[test]
fn noise_floor_over_multiple_pairs_averages_all_pairwise_gaps() {
    let mut report = report_with(None, 0.9);
    report.raw = scores(0.5, None, 0.7);
    report.raw_replicates = vec![scores(0.5, None, 0.6), scores(0.5, None, 0.8)];
    // gaps: |0.7−0.6| = 0.1, |0.7−0.8| = 0.1, |0.6−0.8| = 0.2
    let floor = report.noise_floor().expect("replicates ran");
    assert!((floor - 0.4 / 3.0).abs() < 1e-12, "{floor}");
    assert_eq!(report.noise_pairs(), 3);
    assert_eq!(report.effect_verdict().expect("has drift").pairs(), 3);
}

/// A report written before the multi-pair field existed — populated
/// `raw_replicate`, empty `raw_replicates` — reads exactly as it always
/// did: one gap, one pair.
#[test]
fn a_legacy_single_pair_report_reads_exactly_as_before() {
    let report = report_with_replicate(0.2, 0.05);
    assert!((report.noise_floor().unwrap() - 0.05).abs() < 1e-12);
    assert_eq!(report.noise_pairs(), 1);
    assert_eq!(report.effect_verdict().unwrap().pairs(), 1);
}

/// Pair 1 must reproduce the single-pair seed set, or a multi-pair run's
/// first pair would not be comparable with every run recorded before it.
#[test]
fn the_first_replicate_seed_set_is_the_legacy_one() {
    assert_eq!(
        replicate_seed_set(&DEFAULT_SEEDS, 1),
        replicate_seeds(&DEFAULT_SEEDS)
    );
    let second = replicate_seed_set(&DEFAULT_SEEDS, 2);
    assert_ne!(second, replicate_seeds(&DEFAULT_SEEDS));
    assert_ne!(second, DEFAULT_SEEDS.to_vec());
}

/// The report derives it from the drill-down it already stores, so a
/// legacy report gains the paired view retroactively.
#[test]
fn a_report_derives_its_paired_effect_from_tasks() {
    let mut report = report_with(None, 0.7);
    assert!(report.paired_effect().is_none(), "no drill-down stored");
    report.tasks = vec![paired_task("a", &[0.5], &[1.0])];
    let paired = report.paired_effect().expect("one pair");
    assert_eq!((paired.pairs, paired.wins), (1, 1));
}
