//! Tests for the endpoint's switch at launch on `gglib config`: what `show`
//! says for each position, which is what the service does at its start.

use super::super::unset::handle_unset;
use super::drawing_tests::shown;
use super::tests::key;
use super::*;
use crate::bootstrap::test_context;

/// Unset, the endpoint comes on at launch and `show` says so rather than a
/// bare "None"; `true` and `false` are shown as stored, and `unset` turns it
/// back on.
#[tokio::test]
async fn show_says_what_the_service_does_with_the_switch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    assert_eq!(shown(&ctx, "proxy-autostart").await, "None (on)");

    handle_set(&ctx, key("proxy-autostart"), "false")
        .await
        .expect("off");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert!(!stored.effective_proxy_autostart());
    assert_eq!(shown(&ctx, "proxy-autostart").await, "false");

    handle_set(&ctx, key("proxy-autostart"), "true")
        .await
        .expect("on");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert!(stored.effective_proxy_autostart());
    assert_eq!(shown(&ctx, "proxy-autostart").await, "true");

    handle_unset(&ctx, key("proxy-autostart"))
        .await
        .expect("unset");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.proxy_autostart, None);
    assert!(stored.effective_proxy_autostart());
    assert_eq!(shown(&ctx, "proxy-autostart").await, "None (on)");
}
