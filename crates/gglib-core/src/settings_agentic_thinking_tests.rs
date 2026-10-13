//! Tests for [`AgenticThinking`]: its two defaults, its one rule, its wire
//! spelling, and how a stored one is read, written and cleared.

use super::AgenticThinking::{self, Brief, Off, WhenAsked};
use super::{Settings, SettingsUpdate};
use crate::request_pipeline::LastSpeaker::{Person, Work};

fn settings(policy: Option<AgenticThinking>) -> Settings {
    Settings {
        agentic_thinking: policy,
        ..Settings::with_defaults()
    }
}

/// The type's own default changes nothing; an install with nothing stored
/// runs `when-asked`. They are different answers to different questions.
#[test]
fn the_types_default_changes_nothing_and_an_unset_install_is_when_asked() {
    assert_eq!(AgenticThinking::default(), AgenticThinking::Default);
    for fresh in [Settings::with_defaults(), Settings::default()] {
        assert_eq!(fresh.agentic_thinking, None);
        assert_eq!(fresh.effective_agentic_thinking(), WhenAsked);
    }
}

#[test]
fn the_effective_policy_is_the_stored_one_then_when_asked() {
    for (stored, expected) in [
        (Some(AgenticThinking::Default), AgenticThinking::Default),
        (Some(Off), Off),
        (Some(Brief), Brief),
        (Some(WhenAsked), WhenAsked),
        (None, WhenAsked),
    ] {
        assert_eq!(
            settings(stored).effective_agentic_thinking(),
            expected,
            "stored {stored:?}"
        );
    }
}

/// The whole rule, one row per policy and speaker.
#[test]
fn each_policy_gives_its_budget_by_who_spoke_last() {
    for (policy, after_work, after_person) in [
        (AgenticThinking::Default, None, None),
        (Off, Some(0), Some(0)),
        (Brief, Some(512), Some(512)),
        (WhenAsked, Some(0), None),
    ] {
        assert_eq!(policy.budget_for(Work), after_work, "{policy:?} after work");
        assert_eq!(
            policy.budget_for(Person),
            after_person,
            "{policy:?} after the person"
        );
    }
}

#[test]
fn writing_the_policy_stores_it() {
    let mut s = settings(None);
    s.merge(&SettingsUpdate {
        agentic_thinking: Some(Some(Brief)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.agentic_thinking, Some(Brief));
    assert_eq!(s.effective_agentic_thinking(), Brief);
}

#[test]
fn an_update_that_does_not_name_the_policy_leaves_it_alone() {
    let mut s = settings(Some(Off));
    s.merge(&SettingsUpdate {
        proxy_port: Some(Some(9191)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.agentic_thinking, Some(Off));
}

#[test]
fn clearing_the_policy_returns_to_when_asked() {
    let mut s = settings(Some(Off));
    s.merge(&SettingsUpdate {
        agentic_thinking: Some(None),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.agentic_thinking, None);
    assert_eq!(s.effective_agentic_thinking(), WhenAsked);
}

/// Persisted in settings and generated into TypeScript, and the spelling
/// `gglib config settings set --agentic-thinking` takes.
#[test]
fn the_wire_spelling_is_kebab_case() {
    for (policy, spelling) in [
        (AgenticThinking::Default, "\"default\""),
        (Off, "\"off\""),
        (Brief, "\"brief\""),
        (WhenAsked, "\"when-asked\""),
    ] {
        assert_eq!(serde_json::to_string(&policy).unwrap(), spelling);
        assert_eq!(
            serde_json::from_str::<AgenticThinking>(spelling).unwrap(),
            policy
        );
    }
}

/// A stored policy survives the settings' own serialisation, and a settings
/// value written before the field existed reads as unset.
#[test]
fn a_stored_policy_round_trips_and_an_absent_one_reads_as_unset() {
    let json = serde_json::to_string(&settings(Some(WhenAsked))).unwrap();
    let back: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.agentic_thinking, Some(WhenAsked));

    let mut without = serde_json::to_value(settings(Some(Off))).unwrap();
    without.as_object_mut().unwrap().remove("agentic_thinking");
    let back: Settings = serde_json::from_value(without).unwrap();
    assert_eq!(back.agentic_thinking, None);
}
