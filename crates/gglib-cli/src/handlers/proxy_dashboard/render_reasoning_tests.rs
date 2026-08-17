//! Tests for [`super`] — the reasoning-control section.
//!
//! Nearly every assertion here is about a *distinction* rather than a value:
//! unknown against no, suppressed against sent, a bound that fired against one
//! that never did. That is what the section is for.

use super::super::DEFAULT_TERM_WIDTH;
use super::super::wire_sampling::*;
use super::*;

fn audit(reasoning: Reasoning) -> SamplingAudit {
    SamplingAudit {
        reasoning: Some(reasoning),
        client_field_names: Some(ClientFieldNames::default()),
    }
}

/// **The tri-state's whole point, at the last layer that can lose it.** A
/// template nobody has managed to ask about must not read as one that
/// positively ignores the variable — the first licenses no conclusion, and the
/// second says every effort setting on this model is inert.
#[test]
fn a_template_nobody_has_asked_about_is_not_rendered_as_unsupported() {
    let rendered = render_reasoning_section(
        Some(&audit(Reasoning {
            effort_support: EffortSupport::NotYetObserved {
                reason: "no /props read has completed for the running model yet".to_string(),
            },
            ..Reasoning::default()
        })),
        DEFAULT_TERM_WIDTH,
    );

    assert!(rendered.contains("not observed"), "{rendered}");
    assert!(rendered.contains("/props"), "{rendered}");
    assert!(
        !rendered.contains("suppressed before sending"),
        "{rendered}"
    );
}

/// A state this build cannot read is another way of not having observed one.
/// Falling through to "no" would let a newer proxy silently teach an older
/// dashboard that every effort setting is dead.
#[test]
fn a_state_this_build_does_not_recognise_reads_as_unobserved() {
    let rendered = render_reasoning_section(
        Some(&audit(Reasoning {
            effort_support: EffortSupport::Unrecognised,
            ..Reasoning::default()
        })),
        DEFAULT_TERM_WIDTH,
    );

    assert!(rendered.contains("not observed"), "{rendered}");
}

/// The suppressed marker is what keeps the line honest: without it the
/// dashboard reports a level that was deleted before sending as though
/// llama-server had received it, and no readback can contradict that.
#[test]
fn a_suppressed_level_is_printed_with_its_rung_and_its_marker() {
    let rendered = render_reasoning_section(
        Some(&audit(Reasoning {
            effort_support: EffortSupport::NotSupported,
            latest: Some(ResolvedReasoning {
                effort: Some(EffortRung {
                    level: "high".to_string(),
                    source: "profile".to_string(),
                    suppressed: true,
                }),
                budget: None,
            }),
            wire_blind_reason: "llama-server echoes neither reasoning control.".to_string(),
        })),
        DEFAULT_TERM_WIDTH,
    );

    assert!(rendered.contains("high (profile)"), "{rendered}");
    assert!(rendered.contains("suppressed"), "{rendered}");
    // The blindness is printed beside the value it qualifies.
    assert!(rendered.contains("echoes neither"), "{rendered}");
}

/// **`-1` and `0` are opposite instructions, and one is not the other's
/// fallback.** `-1` defers to the launch `--reasoning-budget`; `0` stops
/// thinking immediately. A `u64` conversion with a `0` fallback printed the
/// first as the second, on the one surface that reports a value nothing echoes.
#[test]
fn a_deferring_budget_is_not_reported_as_stop_thinking_immediately() {
    let rendered = render_reasoning_section(
        Some(&audit(Reasoning {
            effort_support: EffortSupport::Supported,
            latest: Some(ResolvedReasoning {
                effort: None,
                budget: Some(BudgetRung {
                    tokens: -1,
                    source: "global".to_string(),
                }),
            }),
            wire_blind_reason: "llama-server echoes neither reasoning control.".to_string(),
        })),
        DEFAULT_TERM_WIDTH,
    );

    assert!(rendered.contains("-1 tokens (global)"), "{rendered}");
    assert!(!rendered.contains("0 tokens"), "{rendered}");
}

/// The sentinel fix must not cost the grouping every other count on this frame
/// gets: real budgets are four to five digits.
#[test]
fn a_real_budget_keeps_its_thousands_separator() {
    let rendered = render_reasoning_section(
        Some(&audit(Reasoning {
            effort_support: EffortSupport::Supported,
            latest: Some(ResolvedReasoning {
                effort: None,
                budget: Some(BudgetRung {
                    tokens: 32_768,
                    source: "profile".to_string(),
                }),
            }),
            wire_blind_reason: "llama-server echoes neither reasoning control.".to_string(),
        })),
        DEFAULT_TERM_WIDTH,
    );

    assert!(rendered.contains("32,768 tokens (profile)"), "{rendered}");
    // A budget with no effort beside it is still a resolved value, so the
    // blindness note that qualifies it stays on.
    assert!(rendered.contains("echoes neither"), "{rendered}");
}

/// With nothing resolved there is no claim to qualify, so the blindness note
/// stays off — a warning that qualifies nothing is one people learn to skip.
#[test]
fn the_blindness_note_is_printed_only_beside_a_value() {
    let rendered = render_reasoning_section(
        Some(&audit(Reasoning {
            effort_support: EffortSupport::Supported,
            latest: Some(ResolvedReasoning::default()),
            wire_blind_reason: "llama-server echoes neither reasoning control.".to_string(),
        })),
        DEFAULT_TERM_WIDTH,
    );

    assert!(rendered.contains("none resolved"), "{rendered}");
    assert!(!rendered.contains("echoes neither"), "{rendered}");
}

/// A proxy older than this contract sends nothing, which is not the same as a
/// current one reporting that nothing has been resolved.
#[test]
fn a_proxy_that_does_not_report_the_readback_says_so() {
    let rendered = render_reasoning_section(None, DEFAULT_TERM_WIDTH);
    assert!(
        rendered.contains("not reported by this proxy"),
        "{rendered}"
    );
}

/// **The count could not answer the question; the name can.** "Is gglib
/// ignoring the reasoning_effort I sent?" is about one field, and a total is
/// not an answer to it.
#[test]
fn dropped_client_fields_are_listed_by_name_and_by_kind() {
    let rendered = render_client_fields_section(Some(&SamplingAudit {
        reasoning: None,
        client_field_names: Some(ClientFieldNames {
            fields: vec![
                ClientFieldTally {
                    field: "reasoning_effort".to_string(),
                    discarded: 12,
                    rejected: 0,
                },
                ClientFieldTally {
                    field: "top_k".to_string(),
                    discarded: 0,
                    rejected: 2,
                },
            ],
            untracked: 0,
        }),
    }));

    assert!(rendered.contains("reasoning_effort"), "{rendered}");
    assert!(rendered.contains("12 untrusted"), "{rendered}");
    assert!(rendered.contains("2 unreadable"), "{rendered}");
    assert!(!rendered.contains("untracked names"), "{rendered}");
}

/// **Silence is not a clean reading.** A proxy older than this contract sends
/// no readback at all, and printing that as "nothing was dropped" would state a
/// positive fact about a thing nobody measured — the same collapse
/// [`a_proxy_that_does_not_report_the_readback_says_so`] pins one section up.
#[test]
fn a_proxy_that_reports_no_readback_does_not_claim_nothing_was_dropped() {
    let rendered = render_client_fields_section(None);

    assert!(
        rendered.contains("not reported by this proxy"),
        "{rendered}"
    );
    assert!(!rendered.contains("no client sampling field"), "{rendered}");
}

/// The other half of that distinction: a proxy that *does* report, with an
/// empty tally, has measured something and must say so — otherwise the fix
/// above would trade one collapsed state for another.
#[test]
fn a_reporting_proxy_with_an_empty_tally_says_nothing_was_dropped() {
    let audit = SamplingAudit {
        client_field_names: Some(ClientFieldNames::default()),
        ..SamplingAudit::default()
    };
    let rendered = render_client_fields_section(Some(&audit));

    assert!(
        rendered.contains("no client sampling field has been dropped"),
        "{rendered}"
    );
    assert!(
        !rendered.contains("not reported by this proxy"),
        "{rendered}"
    );
}

/// The tally is bounded, and a bound nobody can see is indistinguishable from
/// a bound nobody hit.
#[test]
fn drops_past_the_tallys_bound_are_reported_rather_than_hidden() {
    let rendered = render_client_fields_section(Some(&SamplingAudit {
        reasoning: None,
        client_field_names: Some(ClientFieldNames {
            fields: vec![ClientFieldTally {
                field: "temperature".to_string(),
                discarded: 1,
                rejected: 0,
            }],
            untracked: 7,
        }),
    }));

    assert!(rendered.contains("(untracked names)"), "{rendered}");
    assert!(rendered.contains('7'), "{rendered}");
}
