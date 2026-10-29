//! A shim over a [`ScriptedDecoder`], talked to over a real socket as
//! gglib-proxy talks to it.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use gglib_core::LlmStreamEvent;
use gglib_core::cache_config::KvCacheType;
use gglib_core::sse::SseStreamDecoder;
use serde_json::{Value, json};

use super::{Shim, start};
use crate::decoder::{LoadParams, Script, ScriptRecord, ScriptedDecoder};
use crate::engine::Engine;

/// The context the tests' model is loaded with.
pub(super) const N_CTX: u32 = 256;

/// A shim serving a scripted model, and what its decoder recorded.
pub(super) struct Running {
    pub(super) shim: Shim,
    pub(super) engine: Arc<Engine>,
    record: Arc<Mutex<ScriptRecord>>,
    client: reqwest::Client,
}

/// A shim over `script`, with no model loaded.
pub(super) async fn empty(script: Script) -> Running {
    let (decoder, record) = ScriptedDecoder::new(script);
    let engine = Engine::new(decoder);
    let shim = start(Arc::clone(&engine)).await.expect("the shim starts");
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("a client");
    Running {
        shim,
        engine,
        record,
        client,
    }
}

/// A shim over `script`, its model loaded as `tiny` with [`N_CTX`].
pub(super) async fn running(script: Script) -> Running {
    running_in(script, N_CTX).await
}

/// A shim over `script`, its model loaded as `tiny` with `n_ctx`.
pub(super) async fn running_in(script: Script, n_ctx: u32) -> Running {
    let running = empty(script).await;
    let params = LoadParams {
        n_ctx,
        cache_type_k: KvCacheType::Q8_0,
        cache_type_v: KvCacheType::Q8_0,
    };
    running
        .engine
        .load("tiny", Path::new("/models/tiny.gguf"), &params)
        .expect("the scripted model loads");
    running
}

/// A chat of one user turn saying `text`.
pub(super) fn ask(text: &str) -> Value {
    json!({"model": "tiny", "messages": [{"role": "user", "content": text}]})
}

/// A chat asking to be streamed with progress and usage, as gglib-proxy
/// forwards every chat.
pub(super) fn streamed(text: &str) -> Value {
    let mut body = ask(text);
    body["stream"] = json!(true);
    body["return_progress"] = json!(true);
    body["stream_options"] = json!({"include_usage": true});
    body
}

/// Every event gglib-core's decoder reads from `body`, fed in small pieces.
pub(super) fn decoded(body: &str) -> Vec<LlmStreamEvent> {
    let mut decoder = SseStreamDecoder::default();
    let mut events = Vec::new();
    for piece in body.as_bytes().chunks(7) {
        let (more, stop) = decoder.feed_bytes(piece);
        events.extend(more.into_iter().map(|e| e.expect("a well-formed frame")));
        if stop {
            break;
        }
    }
    events
}

impl Running {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.shim.base_url())
    }

    /// `POST path` with `body`: the status and the body read whole.
    pub(super) async fn post(&self, path: &str, body: &Value) -> (u16, String) {
        let response = self
            .client
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .expect("the shim answers");
        let status = response.status().as_u16();
        (status, response.text().await.expect("a body"))
    }

    /// `POST path` with `body`, the answer unread.
    pub(super) async fn send(&self, path: &str, body: &Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .expect("the shim answers")
    }

    /// `GET path`: the status and the body read whole.
    pub(super) async fn get(&self, path: &str) -> (u16, String) {
        let response = self
            .client
            .get(self.url(path))
            .send()
            .await
            .expect("the shim answers");
        let status = response.status().as_u16();
        (status, response.text().await.expect("a body"))
    }

    /// What the decoder has recorded so far.
    pub(super) fn record(&self) -> ScriptRecord {
        self.record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// `body` as JSON.
pub(super) fn json_of(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"))
}

/// The JSON of each `data:` frame of a streamed body, `[DONE]` left out.
pub(super) fn frames(body: &str) -> Vec<Value> {
    body.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| data.starts_with('{'))
        .map(json_of)
        .collect()
}
