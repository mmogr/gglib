//! Tests for the endpoint's switch at launch: its default, and how a stored
//! one is read, written and cleared.

use super::{Settings, SettingsUpdate};

fn settings(proxy_autostart: Option<bool>) -> Settings {
    Settings {
        proxy_autostart,
        ..Settings::with_defaults()
    }
}

/// A fresh install stores nothing, and its endpoint comes on; only a stored
/// `false` keeps it off.
#[test]
fn the_endpoint_comes_on_at_launch_unless_switched_off() {
    for (stored, expected) in [(Some(false), false), (Some(true), true), (None, true)] {
        assert_eq!(
            settings(stored).effective_proxy_autostart(),
            expected,
            "stored {stored:?}"
        );
    }
    assert!(Settings::default().effective_proxy_autostart());
}

/// `gglib config reset` writes these defaults, so a reset turns it on.
#[test]
fn the_defaults_turn_the_endpoint_on_at_launch() {
    assert_eq!(Settings::with_defaults().proxy_autostart, None);
    assert!(Settings::with_defaults().effective_proxy_autostart());
}

#[test]
fn clearing_the_switch_turns_the_endpoint_back_on_at_launch() {
    let mut s = settings(Some(false));
    s.merge(&SettingsUpdate {
        proxy_autostart: Some(None),
        ..SettingsUpdate::default()
    });

    assert_eq!(s.proxy_autostart, None);
    assert!(s.effective_proxy_autostart());
}
