//! Tests for [`LoopGuardMode`] and the precedence that reconciles it with the
//! boolean it replaces.
//!
//! Their own file because `settings_tests.rs` is at its size baseline. All
//! twelve combinations of the two fields are here, because the pair is the
//! whole compatibility story for one release and "nine" was a miscount of it:
//! the mode has four states (absent, off, note, refuse) and the boolean three
//! (absent, false, true).

use super::{LoopGuardMode, Settings, SettingsUpdate};

fn settings(mode: Option<LoopGuardMode>, bool_setting: Option<bool>) -> Settings {
    Settings {
        loop_guard_mode: mode,
        proxy_loop_detection: bool_setting,
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
fn all_twelve_combinations_of_the_two_spellings_resolve() {
    use LoopGuardMode::{Note, Off, Refuse};
    // (mode, deprecated bool) -> effective mode.
    let cases = [
        // The new setting wins outright whenever it is present, whatever the
        // boolean says — including when they disagree.
        (Some(Off), None, Off),
        (Some(Off), Some(false), Off),
        (Some(Off), Some(true), Off),
        (Some(Note), None, Note),
        (Some(Note), Some(false), Note),
        (Some(Note), Some(true), Note),
        (Some(Refuse), None, Refuse),
        (Some(Refuse), Some(false), Refuse),
        (Some(Refuse), Some(true), Refuse),
        // Absent: the boolean answers, for a settings file an older build
        // wrote. `false` is still off; `true` and absent are now a note, not
        // a refusal — the behaviour change #1052 exists to make.
        (None, Some(false), Off),
        (None, Some(true), Note),
        (None, None, Note),
    ];
    assert_eq!(cases.len(), 12);
    for (mode, bool_setting, expected) in cases {
        assert_eq!(
            settings(mode, bool_setting).effective_loop_guard_mode(),
            expected,
            "mode {mode:?} with proxy_loop_detection {bool_setting:?}"
        );
    }
}

#[test]
fn writing_the_mode_clears_the_deprecated_bool() {
    let mut s = settings(None, Some(false));
    s.merge(&SettingsUpdate {
        loop_guard_mode: Some(Some(LoopGuardMode::Refuse)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard_mode, Some(LoopGuardMode::Refuse));
    assert_eq!(
        s.proxy_loop_detection, None,
        "the two must never disagree on disk"
    );
    assert_eq!(s.effective_loop_guard_mode(), LoopGuardMode::Refuse);
}

#[test]
fn writing_the_deprecated_bool_clears_the_mode() {
    // The half that makes `--proxy-loop-detection false` keep working: without
    // it, anything that had ever written the mode would leave the boolean
    // last in precedence for ever, and the flag would silently do nothing.
    let mut s = settings(Some(LoopGuardMode::Refuse), None);
    s.merge(&SettingsUpdate {
        proxy_loop_detection: Some(Some(false)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.proxy_loop_detection, Some(false));
    assert_eq!(s.loop_guard_mode, None);
    assert_eq!(
        s.effective_loop_guard_mode(),
        LoopGuardMode::Off,
        "the deprecated off-switch still switches the guard off"
    );
}

#[test]
fn an_update_carrying_both_spellings_answers_with_the_new_one() {
    let mut s = Settings::with_defaults();
    s.merge(&SettingsUpdate {
        proxy_loop_detection: Some(Some(false)),
        loop_guard_mode: Some(Some(LoopGuardMode::Refuse)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard_mode, Some(LoopGuardMode::Refuse));
    assert_eq!(s.proxy_loop_detection, None);
}

#[test]
fn clearing_the_mode_returns_to_the_default() {
    let mut s = settings(Some(LoopGuardMode::Off), None);
    s.merge(&SettingsUpdate {
        loop_guard_mode: Some(None),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard_mode, None);
    assert_eq!(s.proxy_loop_detection, None);
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
