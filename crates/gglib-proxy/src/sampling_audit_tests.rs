//! Tests for [`super`] — the sampling readback.

use super::*;
use gglib_core::domain::{FieldSources, InferenceConfig};
use gglib_core::request_pipeline::{FloorClass, LADDER_RUNGS};

/// Captured verbatim from a real `/slots` poll during a generation on
/// the pinned build — `scripts/experiments/sampler_wire_semantics.py`.
/// The literal shape matters more than the values: it is what the parser
/// must survive.
const REAL_SLOT: &str = r#"{
    "temperature": 0.11999999731779099,
    "top_p": 0.949999988079071,
    "top_k": 40,
    "repeat_penalty": 1.0,
    "presence_penalty": 0.0,
    "min_p": 0.05000000074505806,
    "dry_multiplier": 0.0,
    "dry_base": 1.75,
    "mirostat": 0,
    "samplers": ["penalties","dry","top_n_sigma","top_k","typ_p","top_p","min_p","xtc","temperature"]
}"#;

fn decision(resolved: InferenceConfig, sources: FieldSources) -> SamplingDecision {
    SamplingDecision {
        resolved,
        sources,
        layer_names: ["cli", "client", "profile", "model", "global", "auto"],
        floor: FloorClass::Default,
        agentic_turn: false,
        agentic_ceiling_applied: None,
        client_fields_rejected: Vec::new(),
        client_fields_discarded: Vec::new(),
        applied: true,
    }
}

fn all_from(source: ParamSource) -> FieldSources {
    FieldSources {
        temperature: source,
        top_p: source,
        top_k: source,
        max_tokens: source,
        repeat_penalty: source,
        presence_penalty: source,
        min_p: source,
        dynatemp_range: source,
        dynatemp_exponent: source,
        top_n_sigma: source,
        dry_multiplier: source,
        dry_base: source,
        dry_allowed_length: source,
        dry_penalty_last_n: source,
        frequency_penalty: source,
        reasoning_effort: source,
        reasoning_budget_tokens: source,
    }
}

#[test]
fn a_real_slot_params_payload_parses() {
    let p: SlotParams = serde_json::from_str(REAL_SLOT).expect("real payload parses");
    assert_eq!(p.top_k, Some(40.0));
    assert_eq!(p.samplers.as_ref().unwrap().len(), 9);
    assert_eq!(p.samplers.unwrap().last().unwrap(), "temperature");
}

/// The convention `SlotSnapshot` already follows: a field whose *type*
/// changed degrades to `None` rather than failing the whole response.
#[test]
fn a_type_shifted_field_degrades_alone() {
    let p: SlotParams = serde_json::from_str(r#"{"temperature": {"nested": 1}, "top_p": 0.9}"#)
        .expect("one odd field must not fail the parse");
    assert_eq!(p.temperature, None);
    assert_eq!(p.top_p, Some(0.9));
}

#[test]
fn matching_values_do_not_diverge() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let resolved = InferenceConfig {
        temperature: Some(0.12),
        top_k: Some(40),
        min_p: Some(0.05),
        ..Default::default()
    };
    let d = decision(resolved, all_from(ParamSource::Layer(3)));
    assert!(
        compare(&d, &observed).is_empty(),
        "{:?}",
        compare(&d, &observed)
    );
}

#[test]
fn a_changed_value_is_reported_with_its_provenance() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let resolved = InferenceConfig {
        temperature: Some(0.9), // slot says 0.12
        ..Default::default()
    };
    let d = decision(resolved, all_from(ParamSource::Layer(2)));

    let out = compare(&d, &observed);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].field, "temperature");
    assert_eq!(out[0].provenance, "profile");
}

/// The case ADR 0003's deferral makes normal: gglib sends nothing and
/// llama.cpp supplies its own default. That is the design working, and
/// reporting it would make the counter useless the day deferral ships.
#[test]
fn a_value_gglib_never_sent_is_not_a_divergence() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let resolved = InferenceConfig::default(); // nothing resolved
    let d = decision(resolved, all_from(ParamSource::Unset));
    assert!(compare(&d, &observed).is_empty());
}

/// A body that was never an object had nothing written to it, so there
/// is no intent for the wire to disagree with.
#[test]
fn an_unapplied_decision_compares_nothing() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let mut d = decision(
        InferenceConfig {
            temperature: Some(0.9),
            ..Default::default()
        },
        all_from(ParamSource::Layer(0)),
    );
    d.applied = false;
    assert!(compare(&d, &observed).is_empty());
}

/// Float comparison has to survive `f32` -> JSON -> `f64`. `0.05f32`
/// widened is 0.05000000074505806, not 0.05.
#[test]
fn a_widened_f32_does_not_read_as_a_divergence() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let resolved = InferenceConfig {
        min_p: Some(0.05),
        ..Default::default()
    };
    let d = decision(resolved, all_from(ParamSource::Floor));
    assert!(compare(&d, &observed).is_empty());
}

/// The distinction the whole liveness contract exists for.
#[test]
fn blind_is_not_the_same_state_as_zero_divergences() {
    let blind = AuditState::Blind {
        reason: "no params on this build".into(),
    };
    let clean = AuditState::Comparing {
        comparisons: 100,
        divergences: 0,
    };
    assert_ne!(blind, clean);
    assert!(!blind.is_observing());
    assert!(clean.is_observing());
    assert!(!AuditState::NotYetObserved.is_observing());
}

// ── Abstention ────────────────────────────────────────────────────────

fn intent_at(temp: f32) -> SamplingDecision {
    decision(
        InferenceConfig {
            temperature: Some(temp),
            ..Default::default()
        },
        all_from(ParamSource::Layer(3)),
    )
}

/// The measured common case: four concurrent turns resolving identically,
/// which is what the default configuration produces because every
/// compared field comes from the ladder rather than the client. Measured
/// 0 ambiguous polls out of 10 against a real server.
#[test]
fn identical_intents_are_compared_not_skipped() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let intents = vec![intent_at(0.12), intent_at(0.12), intent_at(0.12)];
    let slots = vec![observed.clone(), observed.clone(), observed];

    let out = compare_poll(&intents, &slots);
    assert_eq!(out.comparisons, 3);
    assert_eq!(out.divergences, 0);
    assert_eq!(out.skipped_ambiguous, 0);
}

/// gglib cannot join a slot to the request that filled it, so when the
/// intents in flight disagree an observation cannot be attributed to one.
/// Guessing would produce a divergence that is an artefact of the guess.
#[test]
fn disagreeing_intents_abstain_rather_than_guess() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let intents = vec![intent_at(0.12), intent_at(0.90)];
    let slots = vec![observed.clone(), observed];

    let out = compare_poll(&intents, &slots);
    assert_eq!(out.comparisons, 0, "nothing may be compared");
    assert_eq!(out.divergences, 0);
    assert_eq!(out.skipped_ambiguous, 2, "and the gap is counted");
}

/// `max_tokens` is client-authoritative and varies request to request,
/// while the compared fields do not. Keying ambiguity on the whole
/// decision would abstain on essentially every poll.
#[test]
fn a_differing_max_tokens_alone_is_not_ambiguity() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let mut a = intent_at(0.12);
    let mut b = intent_at(0.12);
    a.resolved.max_tokens = Some(128);
    b.resolved.max_tokens = Some(4096);

    let out = compare_poll(&[a, b], std::slice::from_ref(&observed));
    assert_eq!(out.comparisons, 1);
    assert_eq!(out.skipped_ambiguous, 0);
}

/// **A false positive this used to produce.** llama-server is reachable
/// directly, and `llama::args::sampling` records that as the one
/// population the deleted launch flags ever served. Such a request
/// occupies a slot gglib has no intent for, and comparing it against
/// gglib's own intent invented a divergence with a confident provenance
/// string attached — on the instrument whose only value is being worth
/// believing when it fires.
#[test]
fn a_busy_slot_gglib_cannot_account_for_abstains_rather_than_diverging() {
    let mine: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let someone_elses = SlotParams {
        temperature: Some(1.9),
        ..mine.clone()
    };
    // One gglib request in flight, two slots busy: the second is not ours.
    let out = compare_poll(&[intent_at(0.12)], &[mine, someone_elses]);

    assert_eq!(out.divergences, 0, "must not report a stranger's slot");
    assert_eq!(out.comparisons, 0);
    assert_eq!(out.skipped_ambiguous, 2, "the whole poll is unattributable");
}

/// The surplus is not compared "as far as it goes". Slots arrive in no
/// particular order, so comparing `min(observed, intents)` of them would
/// pick an arbitrary subset — making the false positive rarer without
/// making it less wrong.
#[test]
fn a_surplus_slot_abstains_over_the_whole_poll_not_just_the_extra() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let out = compare_poll(
        &[intent_at(0.12), intent_at(0.12)],
        &[observed.clone(), observed.clone(), observed],
    );

    assert_eq!(out.comparisons, 0, "not two of the three");
    assert_eq!(out.skipped_ambiguous, 3);
}

/// Fewer busy slots than intents is the ordinary case — a request can be
/// queued, or between shaping and reaching a slot — and must still compare.
#[test]
fn fewer_busy_slots_than_intents_still_compares() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let intents = vec![intent_at(0.12), intent_at(0.12), intent_at(0.12)];

    let out = compare_poll(&intents, std::slice::from_ref(&observed));
    assert_eq!(out.comparisons, 1);
    assert_eq!(out.skipped_ambiguous, 0);
}

#[test]
fn no_recorded_intent_compares_nothing() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    let out = compare_poll(&[], std::slice::from_ref(&observed));
    assert_eq!(out.comparisons, 0);
    assert_eq!(
        out.skipped_ambiguous, 0,
        "absence of intent is not ambiguity"
    );
}

#[test]
fn a_real_divergence_is_counted_once_per_slot() {
    let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
    // One intent per busy slot: two turns in flight, both resolving to
    // 0.90 while the slots report 0.12. Fewer intents than slots is the
    // unattributable case below, not this one.
    let intents = vec![intent_at(0.90), intent_at(0.90)];
    let slots = vec![observed.clone(), observed];

    let out = compare_poll(&intents, &slots);
    assert_eq!(out.comparisons, 2);
    assert_eq!(out.divergences, 2);
    assert_eq!(out.found.len(), 2);
    assert_eq!(out.found[0].field, "temperature");
}

// ── Store ─────────────────────────────────────────────────────────────

fn poll(comparisons: u64, divergences: u64, skipped: u64) -> PollOutcome {
    PollOutcome {
        comparisons,
        divergences,
        skipped_ambiguous: skipped,
        found: Vec::new(),
    }
}

#[test]
fn a_fresh_store_has_not_yet_observed() {
    let store = SamplingAuditStore::new();
    assert_eq!(store.state(), AuditState::NotYetObserved);
    assert!(!store.state().is_observing());
}

/// The trap this store exists to avoid: a poll that compared nothing is
/// not evidence of recovery, so it must not clear the latch.
#[test]
fn a_poll_that_compared_nothing_leaves_the_store_blind() {
    let store = SamplingAuditStore::new();
    store.mark_blind("upstream gone");

    store.record_poll(&poll(0, 0, 3));

    assert!(matches!(store.state(), AuditState::Blind { .. }));
    assert_eq!(store.snapshot().skipped_ambiguous, 3);
}

#[test]
fn a_poll_that_compared_something_clears_the_latch() {
    let store = SamplingAuditStore::new();
    store.mark_blind("upstream gone");

    store.record_poll(&poll(2, 1, 0));

    assert_eq!(
        store.state(),
        AuditState::Comparing {
            comparisons: 2,
            divergences: 1
        }
    );
}

/// Abstention lives beside the state, not inside it: an organ that can
/// see but cannot attribute is a different problem from a blind one, and
/// collapsing them would hide which fix is needed.
#[test]
fn abstention_is_reported_without_claiming_blindness() {
    let store = SamplingAuditStore::new();
    store.record_poll(&poll(0, 0, 12));

    assert_eq!(
        store.state(),
        AuditState::NotYetObserved,
        "abstaining is something a sighted organ does"
    );
    assert_eq!(store.snapshot().skipped_ambiguous, 12);
}

#[test]
fn client_field_counters_accumulate_across_requests() {
    let store = SamplingAuditStore::new();
    let mut d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));
    d.client_fields_discarded = vec!["temperature".into(), "top_p".into()];
    d.client_fields_rejected = vec![gglib_core::domain::FieldIssue::Rejected {
        field: "top_k",
        value: "banana".into(),
        expected: "an integer",
    }];

    store.record_intent(&d, None);
    store.record_intent(&d, None);

    let snap = store.snapshot();
    assert_eq!(snap.client_fields_discarded, 4);
    assert_eq!(snap.client_fields_rejected, 2);
}

// =========================================================================
// seed
// =========================================================================

/// **The premise every reproducibility claim rests on.** A seed that was
/// resolved but never reached the sampler would leave a benchmark reading
/// the resulting variance as signal, with nothing on any surface to say
/// otherwise.
#[test]
fn a_seed_that_did_not_reach_the_sampler_diverges() {
    let resolved = InferenceConfig {
        seed: Some(100),
        ..InferenceConfig::default()
    };
    let observed = SlotParams {
        // What llama-server reports when it drew its own seed.
        seed: Some(4_294_967_295.0),
        ..SlotParams::default()
    };

    let found = compare(&decision(resolved, all_from(ParamSource::Unset)), &observed);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].field, "seed");
    assert!((found[0].sent - 100.0).abs() < 1e-9);
}

#[test]
fn a_seed_that_arrived_intact_does_not_diverge() {
    let resolved = InferenceConfig {
        seed: Some(100),
        ..InferenceConfig::default()
    };
    let observed = SlotParams {
        seed: Some(100.0),
        ..SlotParams::default()
    };

    assert!(compare(&decision(resolved, all_from(ParamSource::Unset)), &observed).is_empty());
}

/// An unseeded request has no intent to diverge from, so llama-server
/// drawing its own random seed is the expected observation rather than a
/// fault. Without this the readback would fire on every ordinary request.
#[test]
fn an_unseeded_request_does_not_diverge_on_the_servers_random_seed() {
    let observed = SlotParams {
        seed: Some(4_294_967_295.0),
        ..SlotParams::default()
    };

    assert!(
        compare(
            &decision(InferenceConfig::default(), all_from(ParamSource::Unset)),
            &observed
        )
        .is_empty()
    );
}

// =========================================================================
// Published-vs-sent
// =========================================================================

fn publishing(pairs: &[(&str, &str)]) -> ModelSamplingDefaults {
    let metadata: std::collections::HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    ModelSamplingDefaults::from_metadata(&metadata)
}

/// **The rule this section exists to obey.** An empty field list means
/// either "this model publishes nothing" or "nothing has been resolved
/// yet", and those license opposite conclusions — so the intent count has
/// to be readable separately, exactly as `AuditState` is.
#[test]
fn an_intent_before_any_model_is_known_compares_nothing() {
    let store = SamplingAuditStore::new();

    store.record_intent(
        &decision(
            InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            },
            all_from(ParamSource::Layer(3)),
        ),
        None,
    );

    let published = store.snapshot().published;
    assert_eq!(published.intents, 0, "nothing was compared");
    assert!(published.fields.is_empty());
}

/// The headline case: the model asks for 0.33 and gglib resolves 1.0.
#[test]
fn a_resolved_value_displacing_a_published_one_is_reported_as_an_override() {
    let store = SamplingAuditStore::new();
    store.set_model_sampling(
        "qwen",
        Some(publishing(&[("general.sampling.temp", "0.33")])),
    );

    store.record_intent(
        &decision(
            InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            },
            all_from(ParamSource::Layer(3)),
        ),
        None,
    );

    let published = store.snapshot().published;
    assert_eq!(published.intents, 1);
    assert_eq!(published.fields.len(), 1);
    assert_eq!(published.fields[0].field, "temperature");
    assert_eq!(published.fields[0].key, "general.sampling.temp");
    match published.fields[0].state {
        PublishedOverrideState::Overridden { published, sending } => {
            assert!((published - 0.33).abs() < 1e-9, "{published}");
            assert!((sending - 1.0).abs() < 1e-6, "{sending}");
        }
        ref other => panic!("expected overridden, got {other:?}"),
    }
}

/// gglib naming nothing is what lets the model's value through, and must
/// never read as an override.
#[test]
fn a_value_gglib_never_names_defers_to_the_model() {
    let store = SamplingAuditStore::new();
    store.set_model_sampling(
        "qwen",
        Some(publishing(&[("general.sampling.top_p", "0.71")])),
    );

    store.record_intent(
        &decision(InferenceConfig::default(), all_from(ParamSource::Unset)),
        None,
    );

    let published = store.snapshot().published;
    assert_eq!(
        published.fields[0].state,
        PublishedOverrideState::Deferred { published: 0.71 }
    );
}

/// A model swap must not leave the previous model's comparison on display,
/// nor carry its intent count across.
#[test]
fn a_model_swap_resets_the_comparison() {
    let store = SamplingAuditStore::new();
    store.set_model_sampling(
        "qwen",
        Some(publishing(&[("general.sampling.temp", "0.33")])),
    );
    store.record_intent(
        &decision(
            InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            },
            all_from(ParamSource::Layer(3)),
        ),
        None,
    );
    assert_eq!(store.snapshot().published.intents, 1, "guards the premise");

    store.set_model_sampling("llama", None);

    let published = store.snapshot().published;
    assert_eq!(published.intents, 0);
    assert!(published.fields.is_empty());
}

/// **Two models that publish nothing compare equal.** A value-keyed reset
/// would carry the first model's intent count into the second and report a
/// comparison that never happened for it.
#[test]
fn a_swap_between_two_silent_models_still_resets() {
    let store = SamplingAuditStore::new();
    store.set_model_sampling("qwen", Some(publishing(&[])));
    store.record_intent(
        &decision(InferenceConfig::default(), all_from(ParamSource::Unset)),
        None,
    );
    assert_eq!(store.snapshot().published.intents, 1, "guards the premise");

    store.set_model_sampling("llama", Some(publishing(&[])));

    assert_eq!(store.snapshot().published.intents, 0);
}

/// The poller retries until `/props` reads, so this is called repeatedly
/// within one launch and must not reset the count each time.
#[test]
fn re_setting_the_same_model_is_idempotent() {
    let store = SamplingAuditStore::new();
    let model = publishing(&[("general.sampling.temp", "0.33")]);
    store.set_model_sampling("qwen", Some(model));
    store.record_intent(
        &decision(InferenceConfig::default(), all_from(ParamSource::Unset)),
        None,
    );

    store.set_model_sampling("qwen", Some(model));

    assert_eq!(store.snapshot().published.intents, 1);
}

#[test]
fn recent_divergences_are_bounded_and_keep_the_newest() {
    let store = SamplingAuditStore::new();
    for i in 0..MAX_RECENT_DIVERGENCES + 5 {
        store.record_poll(&PollOutcome {
            comparisons: 1,
            divergences: 1,
            skipped_ambiguous: 0,
            found: vec![Divergence {
                field: "temperature",
                sent: f64::from(u32::try_from(i).unwrap()),
                observed: 0.0,
                provenance: "floor".into(),
            }],
        });
    }

    let snap = store.snapshot();
    assert_eq!(snap.recent_divergences.len(), MAX_RECENT_DIVERGENCES);
    assert!(
        (snap.recent_divergences.last().unwrap().sent - 24.0).abs() < f64::EPSILON,
        "the newest divergence must survive eviction"
    );
}

#[test]
fn the_ladder_width_matches_the_pipeline() {
    let d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));
    assert_eq!(d.layer_names.len(), LADDER_RUNGS);
}

// ── Template-caps storage (ADR 0007) ──────────────────────────────────

use gglib_core::domain::{TemplateCaps, TemplateCapsState};

/// The store holds the caps tri-state beside the baseline, with the same
/// overwrite-on-set discipline: the latest reading wins, whatever it is.
#[test]
fn template_caps_default_to_not_yet_read_and_hold_the_latest_reading() {
    let store = SamplingAuditStore::new();
    assert_eq!(store.template_caps(), TemplateCapsState::NotYetRead);

    let caps = TemplateCaps {
        supports_reasoning_effort: Some(true),
        ..TemplateCaps::default()
    };
    store.set_template_caps(TemplateCapsState::Read { caps: caps.clone() });
    assert_eq!(store.template_caps().caps(), Some(&caps));

    // A model swap whose read fails must replace the stale report with
    // an honest failure, not leave the previous template's caps standing.
    store.set_template_caps(TemplateCapsState::Unreadable {
        reason: "connection refused".into(),
    });
    assert_eq!(store.template_caps().caps(), None);
}

/// Storage only, this PR: the observation must not leak into the
/// dashboard snapshot until the surface PR deliberately adds it.
#[test]
fn the_snapshot_does_not_carry_template_caps_yet() {
    let store = SamplingAuditStore::new();
    store.set_template_caps(TemplateCapsState::Read {
        caps: TemplateCaps::default(),
    });

    let json = serde_json::to_value(store.snapshot()).expect("snapshot serializes");
    assert!(
        json.get("template_caps").is_none(),
        "template caps surfaced in the snapshot before their PR: {json}"
    );
}

// ── Suppressed reasoning effort (ADR 0007 stage 5b) ────────────────────────

use gglib_core::domain::ReasoningEffort;
use gglib_core::request_pipeline::SuppressedEffort;

/// **The record that exists nowhere else.** A suppressed level is deleted from
/// the body before sending, and neither reasoning control is echoed by
/// `/slots.params` or `/props` (finding 7a) — so if this store does not hold
/// it, no surface in the system can ever say that a `:high` profile went
/// nowhere.
#[test]
fn a_suppressed_level_is_recorded_with_the_rung_that_asked_for_it() {
    let store = SamplingAuditStore::new();
    let d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));

    store.record_intent(
        &d,
        Some(&SuppressedEffort {
            level: ReasoningEffort::High,
            // `layer_names` in `decision` is [cli, client, profile, …].
            source: ParamSource::Layer(2),
        }),
    );

    let report = store.snapshot().effort_suppressed;
    assert_eq!(report.requests, 1);
    let latest = report.latest.expect("the suppression is held");
    assert_eq!(latest.level, ReasoningEffort::High);
    assert_eq!(
        latest.source, "profile",
        "the rung is resolved to its name, not left as a ladder index"
    );
}

/// The rung is the half the decision cannot carry: stage 5b overwrites
/// `sources.reasoning_effort` with the suppression marker, so a store that read
/// the rung off the decision would report `suppressed (template)` — true, and
/// useless for finding out whose setting is inert.
#[test]
fn the_rung_survives_the_decisions_own_provenance_being_overwritten() {
    let store = SamplingAuditStore::new();
    let mut d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));
    d.sources.reasoning_effort = ParamSource::SuppressedByTemplate;

    store.record_intent(
        &d,
        Some(&SuppressedEffort {
            level: ReasoningEffort::Max,
            source: ParamSource::Layer(4),
        }),
    );

    let latest = store.snapshot().effort_suppressed.latest.expect("held");
    assert_eq!(latest.source, "global");
    assert_ne!(latest.source, "suppressed (template)");
}

/// The ordinary request — the overwhelming majority — must leave the counter
/// alone and the latest entry empty.
#[test]
fn a_request_with_nothing_suppressed_records_nothing() {
    let store = SamplingAuditStore::new();
    store.record_intent(
        &decision(InferenceConfig::default(), all_from(ParamSource::Unset)),
        None,
    );

    let report = store.snapshot().effort_suppressed;
    assert_eq!(report.requests, 0);
    assert_eq!(report.latest, None);
}

/// Suppression is a property of the model's template, so on a suppressing model
/// it fires on every request that resolves a level. The count has to accumulate
/// while the entry stays the latest one rather than the first.
#[test]
fn repeated_suppressions_accumulate_and_the_latest_wins() {
    let store = SamplingAuditStore::new();
    let d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));

    for level in [ReasoningEffort::Low, ReasoningEffort::Minimal] {
        store.record_intent(
            &d,
            Some(&SuppressedEffort {
                level,
                source: ParamSource::Layer(3),
            }),
        );
    }

    let report = store.snapshot().effort_suppressed;
    assert_eq!(report.requests, 2);
    assert_eq!(report.latest.unwrap().level, ReasoningEffort::Minimal);
}

/// The wire contract the dashboard is written against.
#[test]
fn the_snapshot_carries_the_suppression_for_a_surface_to_render() {
    let store = SamplingAuditStore::new();
    store.record_intent(
        &decision(InferenceConfig::default(), all_from(ParamSource::Unset)),
        Some(&SuppressedEffort {
            level: ReasoningEffort::XHigh,
            source: ParamSource::Layer(2),
        }),
    );

    let json = serde_json::to_value(store.snapshot()).expect("snapshot serializes");
    let report = &json["effort_suppressed"];
    assert_eq!(report["requests"], serde_json::json!(1), "{json}");
    assert_eq!(
        report["latest"]["level"],
        serde_json::json!("xhigh"),
        "{json}"
    );
    assert_eq!(
        report["latest"]["source"],
        serde_json::json!("profile"),
        "{json}"
    );
}
