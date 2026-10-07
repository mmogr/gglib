//! Tests for [`LoopGuardMode`]: its default, its wire spelling, and how a
//! stored one is read, written and cleared.
//!
//! Their own file because `settings_tests.rs` is at its size baseline.

use super::{LoopGuardMode, Settings, SettingsUpdate};

fn settings(mode: Option<LoopGuardMode>) -> Settings {
    Settings {
        loop_guard_mode: mode,
        ..Settings::with_defaults()
    }
}

#[test]
fn the_default_is_a_note() {
    assert_eq!(LoopGuardMode::default(), LoopGuardMode::Note);
    assert_eq!(
        Settings::with_defaults().effective_loop_guard_mode(),
        LoopGuardMode::Note,
        "a fresh install notes rather than refuses"
    );
}

#[test]
fn only_off_stops_the_scan() {
    assert!(!LoopGuardMode::Off.scans());
    assert!(LoopGuardMode::Note.scans());
    assert!(LoopGuardMode::Refuse.scans());
}

#[test]
fn the_effective_mode_is_the_stored_one_then_the_default() {
    use LoopGuardMode::{Note, Off, Refuse};
    for (stored, expected) in [
        (Some(Off), Off),
        (Some(Note), Note),
        (Some(Refuse), Refuse),
        (None, Note),
    ] {
        assert_eq!(
            settings(stored).effective_loop_guard_mode(),
            expected,
            "stored {stored:?}"
        );
    }
}

#[test]
fn writing_the_mode_stores_it() {
    let mut s = settings(None);
    s.merge(&SettingsUpdate {
        loop_guard_mode: Some(Some(LoopGuardMode::Refuse)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard_mode, Some(LoopGuardMode::Refuse));
    assert_eq!(s.effective_loop_guard_mode(), LoopGuardMode::Refuse);
}

#[test]
fn an_update_that_does_not_name_the_mode_leaves_it_alone() {
    let mut s = settings(Some(LoopGuardMode::Off));
    s.merge(&SettingsUpdate {
        proxy_port: Some(Some(9191)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard_mode, Some(LoopGuardMode::Off));
}

#[test]
fn clearing_the_mode_returns_to_the_default() {
    let mut s = settings(Some(LoopGuardMode::Off));
    s.merge(&SettingsUpdate {
        loop_guard_mode: Some(None),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard_mode, None);
    assert_eq!(s.effective_loop_guard_mode(), LoopGuardMode::Note);
}

#[test]
fn the_wire_spelling_is_lowercase() {
    // Persisted in settings files and generated into TypeScript, so the
    // spelling is a contract rather than a detail.
    for (mode, spelling) in [
        (LoopGuardMode::Off, "\"off\""),
        (LoopGuardMode::Note, "\"note\""),
        (LoopGuardMode::Refuse, "\"refuse\""),
    ] {
        assert_eq!(serde_json::to_string(&mode).unwrap(), spelling);
        assert_eq!(
            serde_json::from_str::<LoopGuardMode>(spelling).unwrap(),
            mode
        );
    }
}
