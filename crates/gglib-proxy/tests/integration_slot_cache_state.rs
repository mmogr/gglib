//! What the slot cache is told from outside a chat request: by a clear, and
//! by a load that starts the server.
//!
//! The slot cache remembers which session the server holds in RAM and when
//! that server started, and a chat request trusts both: a session taken to be
//! in RAM skips its restore, and a slot file no older than the server is
//! restored onto it. Each test here changes one of those facts through a
//! route that is not a chat request, and reads the result off the next one.
//! `integration_retry.rs` has the restart a chat request finds for itself.

mod fixtures;

use std::sync::atomic::Ordering;

use reqwest::Client;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use fixtures::common::spawn_mock_upstream_with_slots;
use fixtures::scripted::{
    MODEL, ScriptedRuntime, chat, next_second, plant_slot_file, spawn_cached, target,
};

/// A session cleared between two of its requests restores on the second, and
/// the second's save goes ahead, whether the clear named the session or
/// cleared everything.
///
/// A clear that names a session names no model, and has to forget that
/// session as the one held in RAM all the same. A session still taken to be
/// hot skips its restore, and the restore is what lets it save again.
async fn a_session_restores_and_saves_after_a_clear(clear_names_it: bool) {
    let slot_dir = tempfile::tempdir().unwrap();
    let upstream_cancel = CancellationToken::new();
    let (upstream, actions, _saves, _restores, _) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), slot_dir.path().to_path_buf())
            .await;
    let runtime = ScriptedRuntime::new(vec![target(upstream, false)]);
    let proxy = spawn_cached(runtime, slot_dir.path()).await;
    let session = "cleared-session";

    assert_eq!(chat(&proxy.base, session, false).await.status(), 200);
    assert_eq!(*actions.lock().await, vec![1, 2], "generate→save");

    let mut clear = Client::new().post(format!("{}/v1/proxy/cache/clear", proxy.base));
    if clear_names_it {
        clear = clear.header("X-Gglib-Session-Id", session);
    }
    let cleared = clear.send().await.expect("request");
    assert_eq!(cleared.status(), 200);
    let remaining = std::fs::read_dir(slot_dir.path()).unwrap().count();
    assert_eq!(remaining, 0, "the clear deleted the session's slot file");

    plant_slot_file(slot_dir.path(), session);
    assert_eq!(chat(&proxy.base, session, false).await.status(), 200);
    assert_eq!(
        *actions.lock().await,
        vec![1, 2, 0, 1, 2],
        "after the clear, restore→generate→save: the session is no longer \
         the one in RAM, and its save is not skipped"
    );

    proxy.cancel.cancel();
    upstream_cancel.cancel();
}

#[tokio::test]
async fn a_cleared_session_restores_and_saves_on_its_next_request() {
    a_session_restores_and_saves_after_a_clear(true).await;
}

#[tokio::test]
async fn a_session_restores_and_saves_after_everything_is_cleared() {
    a_session_restores_and_saves_after_a_clear(false).await;
}

/// A load that starts the server is where the slot cache learns of the
/// restart. The chat request after it finds the model running, so its own
/// admission reports nothing, and a slot file an earlier server wrote would
/// be restored onto the fresh one.
#[tokio::test]
async fn a_load_that_starts_the_server_makes_earlier_slot_files_stale() {
    let slot_dir = tempfile::tempdir().unwrap();
    let upstream_cancel = CancellationToken::new();
    let (upstream, actions, _saves, restores, _) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), slot_dir.path().to_path_buf())
            .await;
    // The load's admission starts the server; every one after finds it running.
    let runtime = ScriptedRuntime::new(vec![target(upstream, true), target(upstream, false)]);
    let proxy = spawn_cached(runtime, slot_dir.path()).await;

    // A slot file from before the load, in an earlier second than it.
    plant_slot_file(slot_dir.path(), "saved-before-the-load");
    next_second().await;

    let loaded = Client::new()
        .post(format!("{}/v1/models/{MODEL}/load", proxy.base))
        .send()
        .await
        .expect("request");
    assert_eq!(loaded.status(), 200);
    let body: Value = loaded.json().await.expect("json");
    assert_eq!(body["started"], true, "{body}");

    let before = chat(&proxy.base, "saved-before-the-load", false).await;
    assert_eq!(before.status(), 200);
    assert_eq!(
        restores.load(Ordering::Relaxed),
        0,
        "the file predates the server the load started"
    );
    assert_eq!(*actions.lock().await, vec![1, 2], "generate→save");

    // One written since is the fresh server's own, and is restored.
    plant_slot_file(slot_dir.path(), "saved-after-the-load");
    let after = chat(&proxy.base, "saved-after-the-load", false).await;
    assert_eq!(after.status(), 200);
    assert_eq!(restores.load(Ordering::Relaxed), 1);

    proxy.cancel.cancel();
    upstream_cancel.cancel();
}
