//! A turn on a hub chat goes through this machine's endpoint, the real
//! proxy, as any client's request does: its key is the endpoint's, it is in
//! the endpoint's ledger, a model unloaded between two turns is loaded again,
//! a key the endpoint does not admit fails the run in this machine's own
//! words, and a cancelled turn lets go of the model.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::domain::runs::RunStatus;
use gglib_core::domain::{InferenceConfig, InferenceProfile};
use gglib_core::ports::{RunEvent, RunScope, RunsPort as _};
use gglib_runtime::proxy::ProxyConfig;
use serde_json::Value;
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use super::hub_turn_tests::turn;
use super::start;
use crate::handlers::agent::endpoint_fixture::{Daemon, NAME, daemon, daemon_started, store_key};
use crate::handlers::agent::run_fixture::{device, settled};
use crate::handlers::agent::turn_fixture::REPLY;
use crate::handlers::remote::fake_far::{Fake, far};
use crate::state::AppState;

const KEY: &str = "sk-this-machines-endpoint";

/// A fake llama-server answering every request with one short reply.
pub(super) async fn llama() -> (Arc<Fake>, u16) {
    let (fake, at) = far(200, REPLY).await;
    *fake.content_type.lock().unwrap() = "text/event-stream";
    let port = at
        .server_root()
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    (fake, port)
}

/// The completions the fake llama-server was sent, through the endpoint.
fn completions(fake: &Fake) -> usize {
    let seen = fake.seen.lock().unwrap();
    seen.iter()
        .filter(|request| request.uri == "/v1/chat/completions")
        .count()
}

/// Every frame run `id` logged in `scope`, once it has ended.
pub(super) async fn frames(state: &AppState, scope: &RunScope, id: &str) -> Vec<Value> {
    settled(state).await;
    let mut events = state.runs.events(scope, id, 0).unwrap();
    let mut frames = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(
        Duration::from_secs(2),
        futures_util::StreamExt::next(&mut events),
    )
    .await
    {
        match event {
            RunEvent::Frame { data, .. } => frames.push(serde_json::from_str(&data).unwrap()),
            RunEvent::End(_) => break,
            RunEvent::Preview { .. } => {}
        }
    }
    frames
}

pub(super) fn status(state: &AppState, scope: &RunScope, id: &str) -> RunStatus {
    state
        .runs
        .existing(scope, id)
        .unwrap()
        .expect("the run")
        .status
}

/// Whether `frames` begin by waiting for the catalogue's model to load.
fn waited_for_the_load(frames: &[Value]) -> bool {
    frames.first().is_some_and(|frame| {
        frame["type"] == "waiting" && frame["reason"] == "model_load" && frame["model"] == NAME
    })
}

/// This machine's turn, a device's and a phone-kept chat run all reach the
/// model through the endpoint, which demands its key: each replies, and each
/// is in its ledger, counted by its loop guard alone. The model is loaded
/// first, and the run says so by the model's name.
#[tokio::test]
async fn a_turn_goes_through_the_endpoint_with_its_key_and_is_in_its_ledger() {
    let (fake, port) = llama().await;
    let d = daemon(port, Some(KEY)).await;
    let chat = d.chat().await;
    let local = RunScope::Local;
    let phone = device("phone");

    start(&d.state, local.clone(), "l1", turn(chat, "hi"), None)
        .await
        .unwrap();
    let here = frames(&d.state, &local, "l1").await;
    let other = d.chat().await;
    start(&d.state, phone.clone(), "d1", turn(other, "hi"), None)
        .await
        .unwrap();
    let there = frames(&d.state, &phone, "d1").await;
    // A chat a phone keeps itself, run here with builtins.
    let body = serde_json::json!({
        "model": NAME,
        "messages": [{ "role": "user", "content": "hi" }],
    });
    let query = crate::handlers::agent::chat_turn::ChatQuery::default();
    crate::handlers::agent::chat_turn::start(&d.state, "phone", "c1", body, query)
        .await
        .unwrap();
    let kept = frames(&d.state, &phone, "c1").await;
    let ledger = d.dashboard(Some(KEY)).await;
    d.stop().await;

    assert_eq!(
        status(&d.state, &local, "l1"),
        RunStatus::Completed,
        "{here:?}"
    );
    assert_eq!(
        status(&d.state, &phone, "d1"),
        RunStatus::Completed,
        "{there:?}"
    );
    assert_eq!(
        status(&d.state, &phone, "c1"),
        RunStatus::Completed,
        "{kept:?}"
    );
    assert!(waited_for_the_load(&here), "{here:?}");
    assert_eq!(
        completions(&fake),
        3,
        "each turn's one request was forwarded"
    );
    let recent = ledger["recent_requests"].as_array().expect("the ledger");
    assert_eq!(recent.len(), 3, "{ledger}");
    // The endpoint's loop guard counted them; the loops reported nothing of
    // their own to the same ledger.
    let defects = ledger["per_model_defects"].as_object().expect("per model");
    let requests: u64 = defects
        .values()
        .map(|d| d["requests"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(requests, 3, "{ledger}");
    for counts in defects.values() {
        assert_eq!(
            counts["agent_guard_scanned"].as_u64().unwrap_or(0),
            0,
            "{ledger}"
        );
    }
    assert_eq!(d.loader.in_flight(), 0, "every request let go of the model");
}

/// A turn on a model that is loaded says nothing of a load. Once the model
/// is unloaded, the next turn loads it again, says so, and replies: nothing
/// held it between the two.
#[tokio::test]
async fn a_model_unloaded_between_two_turns_is_loaded_again() {
    let (fake, port) = llama().await;
    let d = daemon(port, None).await;
    let chat = d.chat().await;
    let local = RunScope::Local;

    start(&d.state, local.clone(), "l1", turn(chat, "one"), None)
        .await
        .unwrap();
    let first = frames(&d.state, &local, "l1").await;
    start(&d.state, local.clone(), "l2", turn(chat, "two"), None)
        .await
        .unwrap();
    let loaded = frames(&d.state, &local, "l2").await;
    let before = d.loader.admitted();
    d.loader.unload();
    start(&d.state, local.clone(), "l3", turn(chat, "three"), None)
        .await
        .unwrap();
    let again = frames(&d.state, &local, "l3").await;
    let admitted = d.loader.admitted() - before;
    d.stop().await;

    assert!(waited_for_the_load(&first), "{first:?}");
    assert!(!waited_for_the_load(&loaded), "{loaded:?}");
    assert!(waited_for_the_load(&again), "{again:?}");
    assert_eq!(
        status(&d.state, &local, "l3"),
        RunStatus::Completed,
        "{again:?}"
    );
    assert_eq!(admitted, 2, "the load, then the endpoint's request");
    assert_eq!(completions(&fake), 3);
}

/// The key this machine's endpoint demands was changed a moment ago: the
/// endpoint still admits the old one until its settings cache catches up,
/// and the turn, dialled with the new one, fails saying so in this
/// machine's words, not a paired machine's.
#[tokio::test]
async fn a_key_the_endpoint_does_not_admit_fails_the_run_in_this_machines_words() {
    let (fake, port) = llama().await;
    let d = daemon(port, Some("k-start")).await;
    // Read once with the key it started with, so the endpoint's settings
    // cache holds it while the new one is stored.
    d.dashboard(Some("k-start")).await;
    store_key(&d.state, "k-rotated").await;
    let chat = d.chat().await;
    let local = RunScope::Local;

    start(&d.state, local.clone(), "l1", turn(chat, "hi"), None)
        .await
        .unwrap();
    let said = frames(&d.state, &local, "l1").await;
    d.stop().await;

    assert_eq!(
        status(&d.state, &local, "l1"),
        RunStatus::Failed,
        "{said:?}"
    );
    let error = said
        .iter()
        .rev()
        .find(|frame| frame["type"] == "error")
        .and_then(|frame| frame["message"].as_str())
        .expect("the run says why");
    assert!(
        error.contains("this machine's endpoint did not admit its own key"),
        "{error}"
    );
    assert!(!error.contains("pair again"), "{error}");
    assert_eq!(completions(&fake), 0, "nothing reached the model");
}

/// A llama-server that keeps writing every reply, a word every 50 ms, until
/// the test ends or the reader goes: a turn that is still being written.
async fn writing() -> (u16, Arc<Notify>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let done = Arc::new(Notify::new());
    let end = Arc::clone(&done);
    tokio::spawn(async move {
        loop {
            let Ok(incoming) = gglib_test_http::incoming(&listener).await else {
                return;
            };
            let end = Arc::clone(&end);
            tokio::spawn(async move {
                let Ok((request, mut socket)) = incoming.request().await else {
                    return;
                };
                if request.path != "/v1/chat/completions" {
                    let _ = socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n";
                let first = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n";
                if socket.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                loop {
                    let wrote = tokio::select! {
                        () = end.notified() => return,
                        wrote = socket.write_all(first.as_bytes()) => wrote,
                    };
                    if wrote.is_err() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            });
        }
    });
    (port, done)
}

/// Wait until the run has logged `count` frames.
async fn logged(d: &Daemon, scope: &RunScope, id: &str, count: u32) {
    let waited = tokio::time::timeout(Duration::from_secs(10), async {
        while d.state.runs.existing(scope, id).unwrap().unwrap().last_seq < count {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "the run logged {count} frames");
}

/// A turn cancelled while its reply is being written drops the reply it is
/// reading, so the endpoint ends that request and lets go of the model: no
/// request of the endpoint's holds it once the run has ended.
#[tokio::test]
async fn cancelling_a_turn_lets_the_endpoint_go_of_the_model() {
    let (port, done) = writing().await;
    let d = daemon(port, None).await;
    let chat = d.chat().await;
    let local = RunScope::Local;

    start(&d.state, local.clone(), "l1", turn(chat, "hi"), None)
        .await
        .unwrap();
    // The wait for the load, then the reply's first words.
    logged(&d, &local, "l1", 2).await;
    let writing = d.loader.in_flight();
    d.state.runs.cancel(&local, "l1").unwrap();
    settled(&d.state).await;
    d.released().await;
    d.stop().await;
    done.notify_waiters();

    assert_eq!(writing, 1, "the endpoint's request held the model");
    assert_eq!(status(&d.state, &local, "l1"), RunStatus::Cancelled);
}

/// The load lets go of the model before the turn dials, so by then another
/// client may have swapped it off its port, leaving the port empty or
/// serving another model. The turn is still the chat's model's: it dials
/// with that model's id, the endpoint loads it again, the turn replies, and
/// its reply says the chat's model made it, never the one on the port.
#[tokio::test]
async fn a_model_swapped_off_its_port_before_the_dial_is_still_the_one_sent() {
    let another: (i64, String) = (9_999, "another-model".to_owned());
    for after in [None, Some(another.clone())] {
        let (fake, port) = llama().await;
        let d = daemon(port, None).await;
        let chat = d.chat().await;
        let local = RunScope::Local;
        d.loader.swap_after_load(after.clone());

        start(&d.state, local.clone(), "l1", turn(chat, "hi"), None)
            .await
            .unwrap();
        let said = frames(&d.state, &local, "l1").await;
        d.stop().await;

        let swapped = format!("{after:?}");
        assert_eq!(
            status(&d.state, &local, "l1"),
            RunStatus::Completed,
            "{swapped}: {said:?}"
        );
        let asked = d.loader.asked();
        assert_eq!(asked.len(), 2, "the load, then the endpoint's: {asked:?}");
        let theirs = [another.0.to_string(), another.1.clone()];
        assert!(
            !asked.iter().any(|a| theirs.contains(a)),
            "{swapped}: {asked:?}"
        );
        let made_by = said
            .iter()
            .find(|frame| frame["type"] == "turn_usage")
            .map(|frame| frame["model"].clone());
        assert_eq!(made_by, Some(Value::from(NAME)), "{swapped}: {said:?}");
        assert_eq!(completions(&fake), 1, "{swapped}");
    }
}

/// The `max_tokens` the endpoint's model request reaches llama-server with,
/// for one turn on an endpoint started as `start` says.
async fn sent_limit(start: impl FnOnce(&mut ProxyConfig), profile: Option<&str>) -> Value {
    let (fake, port) = llama().await;
    let d = daemon_started(port, None, start).await;
    if let Some(name) = profile {
        let limited = InferenceProfile {
            name: name.to_owned(),
            description: None,
            config: InferenceConfig {
                max_tokens: Some(700),
                ..InferenceConfig::default()
            },
            list_in_models: false,
        };
        let update = gglib_core::SettingsUpdate {
            inference_profiles: Some(Some(vec![limited])),
            ..gglib_core::SettingsUpdate::default()
        };
        d.state.core.settings().update(update).await.unwrap();
    }
    let chat = d.chat().await;
    start_turn(&d, chat).await;
    d.stop().await;
    let request = fake
        .seen
        .lock()
        .unwrap()
        .iter()
        .find(|request| request.uri == "/v1/chat/completions")
        .map(|request| request.body.clone())
        .expect("the turn reached the model");
    let body: Value = serde_json::from_str(&request).unwrap();
    body["max_tokens"].clone()
}

async fn start_turn(d: &Daemon, chat: i64) {
    let local = RunScope::Local;
    start(&d.state, local.clone(), "l1", turn(chat, "hi"), None)
        .await
        .unwrap();
    let said = frames(&d.state, &local, "l1").await;
    assert_eq!(
        status(&d.state, &local, "l1"),
        RunStatus::Completed,
        "{said:?}"
    );
}

/// The turn's limit never outranks what the endpoint's own ladder names: the
/// sampling it was started with (`gglib serve --max-tokens`) and its
/// default profile (`gglib serve --profile`) reach the model. Started with
/// neither, the model gets the turn's limit.
#[tokio::test]
async fn the_endpoints_own_limit_outranks_the_turns() {
    let started = |config: &mut ProxyConfig| {
        config.inference_override = Some(InferenceConfig {
            max_tokens: Some(512),
            ..InferenceConfig::default()
        });
    };
    assert_eq!(sent_limit(started, None).await, Value::from(512));

    let profiled = |config: &mut ProxyConfig| config.default_profile = Some("short".to_owned());
    assert_eq!(sent_limit(profiled, Some("short")).await, Value::from(700));

    assert_eq!(sent_limit(|_| {}, None).await, Value::from(16_384));
}
