//! A request whose upstream turned out to be dead is attempted again, and the
//! second attempt is the first one run again.
//!
//! The admission queue can hand back a server that has since died. The
//! handler then stops what is left of it, asks to be admitted again, and
//! forwards a second time. Each test here stages that with a scripted runtime
//! and pins something the second attempt does because the first does: it goes
//! through the slot cache's restore and save, records the server's fresh
//! start, writes the dashboard's cache and launch entries, and, when the
//! fresh server is dead too, refuses with the Retry-After every other 503
//! carries.

mod fixtures;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::{CacheRamHealth, LaunchNarration};
use gglib_core::ports::ModelRuntimePort;
use gglib_core::retry::RetryPolicy;
use gglib_proxy::slots::slot_bin_path;

use fixtures::common::{
    spawn_mock_upstream, spawn_mock_upstream_with_slots, spawn_mock_upstream_with_slots_streaming,
    spawn_proxy_with_runtime,
};
use fixtures::loop_guard::dashboard_of;
use fixtures::scripted::{
    MODEL, ScriptedRuntime, chat, dead_port, eventually, next_second, plant_slot_file,
    spawn_cached, target,
};

/// One streamed reply, as llama-server sends it.
const CHAT_STREAM: &[u8] =
    b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"index\":0}]}\n\ndata: [DONE]\n\n";

/// [`chat`], answered 200 and read to its end: a streamed reply's save
/// follows its last frame.
async fn chat_ok(proxy: &str, session: &str, streaming: bool) {
    let response = chat(proxy, session, streaming).await;
    assert_eq!(response.status(), 200);
    response.bytes().await.expect("the reply's body");
}

/// A session is saved on a server that then dies. Its next request is
/// admitted to the dead server, admitted again to a fresh one, and attempted
/// there: restored, generated and saved, with the restart recorded.
async fn a_retry_restores_saves_and_records_the_restart(streaming: bool) {
    let slot_dir = tempfile::tempdir().unwrap();
    let upstream_cancel = CancellationToken::new();
    let dir = slot_dir.path().to_path_buf();
    let (live, actions, saves, restores, _) = if streaming {
        spawn_mock_upstream_with_slots_streaming(upstream_cancel.clone(), dir).await
    } else {
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), dir).await
    };
    let runtime = ScriptedRuntime::new(vec![
        target(live, false),
        // The server has died, and this admission does not know it.
        target(dead_port().await, false),
        // The retry's admission is the one that starts it again.
        target(live, true),
        target(live, false),
    ]);
    let proxy = spawn_cached(Arc::clone(&runtime), slot_dir.path()).await;
    let upstream_saved = |n: u64| saves.load(Ordering::Relaxed) >= n;

    // The first server saves "kept", which leaves it the session the proxy
    // takes to be in RAM. It has a slot file for "older" as well.
    let kept = slot_bin_path(slot_dir.path(), 1, "kept");
    chat_ok(&proxy.base, "kept", streaming).await;
    eventually("the first save", || upstream_saved(1) && kept.exists()).await;
    plant_slot_file(slot_dir.path(), "older");

    // The restart lands in a later second than both files. "kept"'s is then
    // dated after it, so the mtime guard lets that one through and nothing
    // but a stale hot marker could skip its restore.
    next_second().await;
    std::fs::File::open(&kept)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_hours(1))
        .unwrap();

    chat_ok(&proxy.base, "kept", streaming).await;
    assert_eq!(runtime.admits(), 3, "admitted, found dead, admitted again");
    assert_eq!(
        restores.load(Ordering::Relaxed),
        1,
        "nothing is in a fresh server's RAM, so the retry restores from disk"
    );
    eventually("the retry's save", || upstream_saved(2)).await;
    assert_eq!(
        *actions.lock().await,
        vec![1, 2, 0, 1, 2],
        "generate→save, then restore→generate→save on the fresh server"
    );

    // The restart was recorded: "older"'s file is an earlier server's now.
    chat_ok(&proxy.base, "older", streaming).await;
    eventually("the third save", || upstream_saved(3)).await;
    assert_eq!(
        restores.load(Ordering::Relaxed),
        1,
        "a slot file from before the restart is not restored onto the fresh server"
    );

    proxy.cancel.cancel();
    upstream_cancel.cancel();
}

#[tokio::test]
async fn a_streaming_retry_restores_saves_and_records_the_restart() {
    a_retry_restores_saves_and_records_the_restart(true).await;
}

#[tokio::test]
async fn a_non_streaming_retry_restores_saves_and_records_the_restart() {
    a_retry_restores_saves_and_records_the_restart(false).await;
}

/// The dashboard's cache and launch entries are written from the target a
/// request was admitted to, and the fresh server's target is not the dead
/// one's.
#[tokio::test]
async fn a_retry_writes_the_fresh_servers_dashboard_entries() {
    let upstream_cancel = CancellationToken::new();
    let live = spawn_mock_upstream(vec![CHAT_STREAM], upstream_cancel.clone()).await;
    let launched_as =
        |quantization: &str| LaunchNarration::new(MODEL, Some(quantization.into()), 0);
    let runtime = ScriptedRuntime::new(vec![
        target(dead_port().await, false).with_narration(launched_as("Q4_K_M")),
        target(live, true)
            .with_cache_ram_health(CacheRamHealth::Low { mb: 64 })
            .with_narration(launched_as("Q8_0")),
    ]);
    let (proxy, proxy_cancel) = spawn_proxy_with_runtime(
        Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
        MODEL,
        vec![],
    )
    .await;

    chat_ok(&proxy, "any", true).await;
    assert_eq!(runtime.admits(), 2, "the first admission was dead");

    let dashboard = dashboard_of(&proxy).await;
    assert_eq!(
        dashboard["cache"]["ram_state"], "low",
        "the entry is the fresh server's: {dashboard}"
    );
    assert_eq!(
        dashboard["launch"]["quantization"], "Q8_0",
        "the launch is the fresh server's: {dashboard}"
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// A fresh server that is dead as well ends the request, with the 503 a model
/// still loading gets and the Retry-After the retry policy gives.
#[tokio::test]
async fn a_retry_that_finds_the_upstream_dead_again_answers_503_with_the_policys_retry_after() {
    let runtime = ScriptedRuntime::new(vec![target(dead_port().await, false)]);
    let (proxy, proxy_cancel) = spawn_proxy_with_runtime(
        Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
        MODEL,
        vec![],
    )
    .await;

    let response = chat(&proxy, "any", true).await;
    assert_eq!(response.status(), 503);
    let advertised = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body: Value = response.json().await.expect("json error body");

    assert_eq!(runtime.admits(), 2, "one retry, and no more");
    assert_eq!(body["error"]["code"], "model_loading", "{body}");
    assert_eq!(
        advertised,
        Some(RetryPolicy::default().max_backoff.as_secs().to_string()),
        "the hint an admission's 503 carries, not one of this path's own"
    );

    proxy_cancel.cancel();
}
