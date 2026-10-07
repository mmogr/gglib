//! Installing the starter profiles through [`super::SettingsOps`].
//!
//! A sibling of `settings_tests.rs`, which is at its size budget; it borrows
//! that module's two builders.

use super::tests::{make_ops, profile};
use super::*;
use crate::test_support::{MockSystemProbePort, test_core};

/// The settings page's install: the nine starter profiles, and a profile the
/// user already has under one of their names left exactly as it was.
#[tokio::test]
async fn installing_the_templates_adds_the_nine_and_keeps_a_profile_of_the_same_name() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());
    let mine = UpdateSettingsRequest {
        inference_profiles: Some(Some(vec![profile("chat", 0.123)])),
        ..Default::default()
    };
    ops.update(mine).await.expect("update should succeed");

    let done = ops.install_profile_templates().await.expect("installs");

    assert_eq!(done.kept, ["chat"]);
    assert_eq!(
        done.installed,
        [
            "coding", "creative", "minimal", "low", "medium", "high", "xhigh", "max"
        ]
    );
    let stored = ops.get().await.unwrap().inference_profiles.unwrap();
    assert_eq!(stored, done.settings.inference_profiles.unwrap());
    assert_eq!(stored.len(), 9);
    assert_eq!(stored[0], profile("chat", 0.123), "the stored chat changed");
}
