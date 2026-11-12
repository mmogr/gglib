//! Tests for the loop guard's switch: its default, and how a stored one is
//! read, written and cleared.

use super::{Settings, SettingsUpdate};

fn settings(loop_guard: Option<bool>) -> Settings {
    Settings {
        loop_guard,
        ..Settings::with_defaults()
    }
}

#[test]
fn the_guard_is_on_unless_switched_off() {
    for (stored, expected) in [(Some(false), false), (Some(true), true), (None, true)] {
        assert_eq!(
            settings(stored).effective_loop_guard(),
            expected,
            "stored {stored:?}"
        );
    }
}

#[test]
fn writing_the_switch_stores_it() {
    let mut s = settings(None);
    s.merge(&SettingsUpdate {
        loop_guard: Some(Some(false)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard, Some(false));
    assert!(!s.effective_loop_guard());
}

#[test]
fn an_update_that_does_not_name_the_switch_leaves_it_alone() {
    let mut s = settings(Some(false));
    s.merge(&SettingsUpdate {
        proxy_port: Some(Some(9191)),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard, Some(false));
}

#[test]
fn clearing_the_switch_turns_the_guard_back_on() {
    let mut s = settings(Some(false));
    s.merge(&SettingsUpdate {
        loop_guard: Some(None),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.loop_guard, None);
    assert!(s.effective_loop_guard());
}
