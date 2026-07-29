//! Integration coverage for the mtime-guard stale-skip path (#597).
//!
//! `slots::slot_file_is_stale` + its call site in
//! `cache_lifecycle::restore_with_retry` skip a restore when a saved slot
//! `.bin` file's mtime predates the current server's `server_start_time` —
//! the file was written by some earlier llama-server instance and can't be
//! trusted. This is already covered at the unit level (`cache_lifecycle.rs`'s
//! `test_restore_with_retry_skips_stale_slot_file` and `slots.rs`'s
//! `slot_file_is_stale` tests), but nothing exercises it end-to-end through a
//! real `gglib_proxy::serve()` instance. This file closes that gap: it proves
//! the guard's fail-open contract holds over HTTP — the stale restore is
//! skipped, no error reaches the client, and the request completes normally.

mod fixtures;

use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use reqwest::Client;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use fixtures::common::{spawn_mock_upstream_with_slots, spawn_proxy_with_cache};
use gglib_proxy::slots::slot_bin_path;

/// A slot file whose mtime predates the proxy's `server_start_time` must be
/// treated as stale: restore is skipped (no network call to the upstream's
/// `/slots/0?action=restore`), and the request still completes successfully
/// — fail-open, per the guard's doc contract in `slots::slot_file_is_stale`.
#[tokio::test]
async fn stale_slot_file_skips_restore_and_fails_open() {
    let slot_dir =
        std::env::temp_dir().join(format!("gglib-slot-stale-mtime-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&slot_dir);

    let upstream_cancel = CancellationToken::new();
    let (upstream_port, action_log, save_count, restore_count, _last_chat_body) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), slot_dir.clone()).await;

    let session_id = "stale-mtime-session";

    // `serve()` stamps `server_start_time` with `now()` at this call.
    let (proxy_base, proxy_cancel) =
        spawn_proxy_with_cache(upstream_port, "test-model", slot_dir.clone()).await;

    // Write the slot file after the proxy starts, then explicitly back-date
    // its mtime well before `server_start_time` — this is what actually
    // proves staleness, rather than relying on the write happening to land
    // before startup (which the mtime guard's whole-second comparison would
    // otherwise race).
    let bin_path = slot_bin_path(&slot_dir, 1, session_id);
    std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
    std::fs::write(&bin_path, b"fake kv state").unwrap();
    let stale_mtime = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::open(&bin_path)
        .unwrap()
        .set_modified(stale_mtime)
        .unwrap();

    let response = Client::new()
        .post(format!("{}/v1/chat/completions", proxy_base))
        .header("X-Gglib-Session-Id", session_id)
        .json(&json!({
            "model": "test-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": false
        }))
        .send()
        .await
        .expect("proxy should be running");

    assert!(
        response.status().is_success(),
        "a stale-mtime restore skip must fail open — the request should still succeed, got: {}",
        response.status()
    );

    assert_eq!(
        restore_count.load(Ordering::Relaxed),
        0,
        "the mtime guard should skip restore before any network call reaches the upstream"
    );
    assert_eq!(
        save_count.load(Ordering::Relaxed),
        1,
        "generation should still proceed and save, even though restore was skipped"
    );

    let actions = action_log.lock().await.clone();
    assert_eq!(
        actions,
        vec![1, 2],
        "expected generate→save only (no restore `0` — the file was skipped as stale), got: {:?}",
        actions
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let _ = std::fs::remove_dir_all(&slot_dir);
}
