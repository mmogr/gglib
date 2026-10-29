//! A served engine over a scripted decoder, driven as the app drives one:
//! serving and shutting down, again and again; the watch; the bearer; a
//! streamed reply through the proxy, carrying the context the engine loaded;
//! a reading no budget can be made from; serving again, over a server held
//! and over one let go of mid-load; a server let go of.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::Utc;
use gglib_core::NewModel;
use gglib_engine_llama::{Script, ScriptRecord, ScriptedDecoder, linked_llama_decoder};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::error::Elapsed;

use crate::library::Library;
use crate::test_hub::TestHub;
use crate::test_library::{PHONE, library_over, wait};
use crate::{DeviceReading, EngineState, EngineWatch, GglibError};

use super::*;

const MIB: u64 = 1 << 20;

/// A phone with 8 GiB, 128 MiB of it free, so a budget of 128 MiB: beside
/// the 64 MiB of `small`'s weights, room for 24,672 tokens of its cache at
/// 2176 bytes a token, so the rung below, 16,384.
const TIGHT: DeviceReading = DeviceReading {
    physical_bytes: 8 * 1024 * MIB,
    available_bytes: 128 * MIB,
    is_simulator: false,
};

fn scripted() -> ScriptedDecoder {
    ScriptedDecoder::new(Script::new(["Hi", "."])).0
}

/// A library in `root` holding `small`: 64 MiB of weights on disk, and the
/// shape its fit reads.
fn library_with_small(root: &TempDir) -> Arc<Library> {
    let library = library_over(TestHub::default(), root);
    let weights = root.path().join("models").join("small.gguf");
    std::fs::File::create(&weights)
        .and_then(|file| file.set_len(64 * MIB))
        .expect("the weights are written");
    let mut small = NewModel::new("small".to_owned(), weights, 0.1, Utc::now());
    small.architecture = Some("llama".to_owned());
    small.context_length = Some(32_768);
    small.metadata = HashMap::from(
        [
            ("llama.block_count", "4"),
            ("llama.attention.head_count", "4"),
            ("llama.attention.head_count_kv", "4"),
            ("llama.attention.key_length", "64"),
            ("llama.attention.value_length", "64"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned())),
    );
    wait(library.models().insert(&small)).expect("the model is added");
    library
}

fn serve(library: &Library, reading: DeviceReading) -> Arc<Server> {
    wait(library.serve_over(scripted(), reading)).expect("the engine is served")
}

fn shut_down(server: &Arc<Server>) {
    wait(Arc::clone(server).shutdown());
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("a client")
}

/// `GET /v1/models` with the server's key: its status, or `None` when
/// nothing answers.
fn models_status(base_url: &str, key: &str) -> Option<u16> {
    wait(async {
        let sent = client()
            .get(format!("{base_url}/models"))
            .bearer_auth(key)
            .send()
            .await;
        sent.ok().map(|response| response.status().as_u16())
    })
}

/// A streamed chat with `small`, asking for progress as the app asks, with
/// `key` as its bearer when there is one: the status and the body.
fn chat(server: &Server, key: Option<&str>) -> (u16, String) {
    wait(async {
        let mut request = client()
            .post(format!("{}/chat/completions", server.base_url()))
            .json(&json!({
                "model": "small",
                "stream": true,
                "return_progress": true,
                "messages": [{"role": "user", "content": "Hello?"}],
            }));
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.expect("the proxy answers");
        let status = response.status().as_u16();
        (status, response.text().await.expect("a body"))
    })
}

/// The next answer, or `Err` when none comes within `within`.
fn next_within(watch: &Arc<EngineWatch>, within: Duration) -> Result<Option<EngineState>, Elapsed> {
    let next = Arc::clone(watch).next();
    wait(async { tokio::time::timeout(within, next).await })
}

fn next(watch: &Arc<EngineWatch>) -> Option<EngineState> {
    next_within(watch, Duration::from_secs(10)).expect("the watch answers")
}

/// What a scripted decoder has been asked and done so far.
fn read(record: &std::sync::Mutex<ScriptRecord>) -> ScriptRecord {
    record
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

#[test]
fn an_engine_is_served_and_shut_down_twice_each_time_with_a_port_and_key_of_its_own() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let mut keys = Vec::new();

    for _ in 0..2 {
        let server = serve(&library, PHONE);
        let base_url = server.base_url();
        let port = base_url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.strip_suffix("/v1"))
            .and_then(|port| port.parse::<u16>().ok());
        assert!(port.is_some(), "{base_url}");
        let key = server.api_key();
        assert_eq!(key.len(), 64, "32 bytes, as hex");
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()), "{key}");
        assert_eq!(models_status(&base_url, &key), Some(200));

        shut_down(&server);
        shut_down(&server);

        assert_eq!(models_status(&base_url, &key), None, "nothing listens");
        keys.push(key);
    }
    assert_ne!(keys[0], keys[1], "each serve has a key of its own");
}

#[test]
fn a_watch_answers_where_the_engine_stands_and_ends_with_the_server() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let server = serve(&library, PHONE);
    let watch = server.status();

    assert_eq!(next(&watch), Some(EngineState::Idle));
    shut_down(&server);

    assert_eq!(next(&watch), Some(EngineState::Stopped));
    assert_eq!(next(&watch), None);
    assert_eq!(next(&watch), None);
    let late = server.status();
    assert_eq!(next(&late), Some(EngineState::Stopped));
    assert_eq!(next(&late), None);
}

#[test]
fn a_chat_without_the_serves_key_is_401_and_loads_nothing() {
    let root = TempDir::new().unwrap();
    let library = library_with_small(&root);
    let server = serve(&library, TIGHT);
    let watch = server.status();
    assert_eq!(next(&watch), Some(EngineState::Idle));

    let (missing, _) = chat(&server, None);
    let (wrong, _) = chat(&server, Some("another-key"));

    assert_eq!(missing, 401);
    assert_eq!(wrong, 401);
    assert!(
        next_within(&watch, Duration::from_millis(200)).is_err(),
        "nothing was loaded for a request refused at the door"
    );
    shut_down(&server);
}

#[test]
fn a_streamed_reply_reports_the_context_the_engine_loaded_its_model_at() {
    let root = TempDir::new().unwrap();
    let library = library_with_small(&root);
    let server = serve(&library, TIGHT);
    let watch = server.status();

    let (status, body) = chat(&server, Some(&server.api_key()));

    assert_eq!(status, 200, "{body}");
    assert!(body.trim_end().ends_with("data: [DONE]"), "{body}");
    let frames: Vec<Value> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| data.starts_with('{'))
        .map(|data| serde_json::from_str(data).expect("a JSON frame"))
        .collect();
    let answer: String = frames
        .iter()
        .filter_map(|f| f.pointer("/choices/0/delta/content")?.as_str())
        .collect();
    assert_eq!(answer, "Hi.");
    let usage = frames
        .iter()
        .find_map(|f| f.get("usage"))
        .unwrap_or_else(|| panic!("a usage frame in {body}"));
    assert_eq!(usage["context_size"], json!(16_384), "{usage}");
    let mut shown = next(&watch);
    while matches!(shown, Some(EngineState::Idle | EngineState::Loading { .. })) {
        shown = next(&watch);
    }
    assert_eq!(
        shown,
        Some(EngineState::Ready {
            model: "small".to_owned(),
            n_ctx: 16_384
        }),
        "the watch's context is the one the reply reported"
    );
    shut_down(&server);
}

#[test]
fn a_reading_with_a_zero_in_it_is_refused_and_leaves_the_engine_served_before() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let before = serve(&library, PHONE);
    let failed = DeviceReading {
        available_bytes: 0,
        ..PHONE
    };

    let served = wait(library.serve_over(scripted(), failed));

    match served {
        Err(GglibError::Internal { message }) => assert!(message.contains("reading"), "{message}"),
        other => panic!("served on a failed reading: {other:?}"),
    }
    assert_eq!(
        models_status(&before.base_url(), &before.api_key()),
        Some(200),
        "the engine served before still serves"
    );
    shut_down(&before);
}

#[test]
fn serving_again_stops_the_engine_served_before() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let first = serve(&library, PHONE);
    let first_watch = first.status();
    assert_eq!(next(&first_watch), Some(EngineState::Idle));

    let second = serve(&library, PHONE);

    assert_eq!(next(&first_watch), Some(EngineState::Stopped));
    assert_eq!(models_status(&first.base_url(), &first.api_key()), None);
    assert_eq!(
        models_status(&second.base_url(), &second.api_key()),
        Some(200)
    );
    shut_down(&second);
}

#[test]
fn serving_again_waits_for_the_load_of_a_server_let_go_of_to_stop() {
    let root = TempDir::new().unwrap();
    let library = library_with_small(&root);
    let (decoder, first_record) = ScriptedDecoder::new(Script {
        load_delay: Duration::from_secs(1),
        load_steps: 4,
        ..Script::new(["Hi", "."])
    });
    let first = wait(library.serve_over(decoder, TIGHT)).expect("the engine is served");
    let watch = first.status();
    let (base_url, key) = (first.base_url(), first.api_key());
    // Its answer is not read: the engine stops before it has one.
    let asking = std::thread::spawn(move || {
        wait(async {
            let _ = client()
                .post(format!("{base_url}/chat/completions"))
                .bearer_auth(key)
                .json(&json!({"model": "small", "messages": [{"role": "user", "content": "Hi"}]}))
                .send()
                .await;
        });
    });
    let mut shown = next(&watch);
    while shown == Some(EngineState::Idle) {
        shown = next(&watch);
    }
    assert!(
        matches!(shown, Some(EngineState::Loading { .. })),
        "{shown:?}"
    );

    drop(first);
    let second = serve(&library, TIGHT);

    let first_record = read(&first_record);
    assert_eq!(first_record.loads.len(), 1);
    assert_eq!(
        first_record.loads_aborted + first_record.unloads,
        1,
        "the first engine's load stopped, and freed what it read, before the second served: \
         {first_record:?}"
    );
    asking.join().expect("the request ends");
    shut_down(&second);
}

#[test]
fn a_server_let_go_of_without_a_shutdown_stops() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let server = serve(&library, PHONE);
    let (base_url, key) = (server.base_url(), server.api_key());

    drop(server);

    let deadline = Instant::now() + Duration::from_secs(5);
    while models_status(&base_url, &key).is_some() {
        assert!(Instant::now() < deadline, "the proxy still answers");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Every build but the framework's for an Apple platform; a build that
/// linked llama.cpp has nothing to show here.
#[test]
fn a_build_without_llama_cpp_cannot_serve() {
    if linked_llama_decoder(None).is_some() {
        return;
    }
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);

    let served = wait(Arc::clone(&library).serve(PHONE));

    assert!(
        matches!(served, Err(GglibError::EngineUnavailable)),
        "{served:?}"
    );
}
