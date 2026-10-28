//! The JSON llama-server answers a chat completion with at the pin
//! (`tools/server/server-task.cpp`, `server-common.cpp`), field for field.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::domain::engine_pins::PINNED_LLAMA_RELEASE;
use serde_json::{Map, Value, json};

use crate::decoder::PromptProgress;

/// What every chunk of one reply, and its unary answer, carries alike.
#[derive(Debug, Clone)]
pub(crate) struct Envelope {
    id: String,
    model: String,
    created: u64,
}

impl Envelope {
    /// A reply from the model served as `model`, with a fresh id.
    pub(crate) fn new(model: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self {
            id: format!("chatcmpl-{:x}{n:08x}", now.as_nanos()),
            model: model.to_owned(),
            created: now.as_secs(),
        }
    }

    fn base(&self, object: &str) -> Map<String, Value> {
        let mut base = Map::new();
        base.insert("created".to_owned(), json!(self.created));
        base.insert("id".to_owned(), json!(self.id));
        base.insert("model".to_owned(), json!(self.model));
        base.insert("system_fingerprint".to_owned(), json!(PINNED_LLAMA_RELEASE));
        base.insert("object".to_owned(), json!(object));
        base
    }

    fn chunk(&self, delta: &Value, finish_reason: Option<&str>) -> Map<String, Value> {
        let mut chunk = self.base("chat.completion.chunk");
        chunk.insert(
            "choices".to_owned(),
            json!([{"finish_reason": finish_reason, "index": 0, "delta": delta}]),
        );
        chunk
    }

    /// The chunk that opens a reply, and that each prompt-progress report
    /// rides on.
    pub(crate) fn role(&self, progress: Option<(PromptProgress, u64)>) -> Value {
        let mut chunk = self.chunk(&json!({"role": "assistant", "content": null}), None);
        if let Some((p, time_ms)) = progress {
            chunk.insert(
                "prompt_progress".to_owned(),
                json!({"total": p.total, "cache": p.cache, "processed": p.processed, "time_ms": time_ms}),
            );
        }
        Value::Object(chunk)
    }

    /// A chunk of the reply's answer.
    pub(crate) fn content(&self, text: &str) -> Value {
        Value::Object(self.chunk(&json!({"content": text}), None))
    }

    /// A chunk of the reply's reasoning.
    pub(crate) fn reasoning(&self, text: &str) -> Value {
        Value::Object(self.chunk(&json!({"reasoning_content": text}), None))
    }

    /// The chunk that says why the reply ended, with the timings when no
    /// usage chunk follows it.
    pub(crate) fn finish(&self, reason: &str, timings: Option<Value>) -> Value {
        let mut chunk = self.chunk(&json!({}), Some(reason));
        if let Some(timings) = timings {
            chunk.insert("timings".to_owned(), timings);
        }
        Value::Object(chunk)
    }

    /// The last chunk when the client asked for usage: no choices.
    pub(crate) fn usage(&self, usage: Value, timings: Value) -> Value {
        let mut chunk = self.base("chat.completion.chunk");
        chunk.insert("choices".to_owned(), json!([]));
        chunk.insert("usage".to_owned(), usage);
        chunk.insert("timings".to_owned(), timings);
        Value::Object(chunk)
    }

    /// The whole reply, for a client that did not stream: its
    /// `reasoning_content` only when it reasoned, as `to_json_oaicompat`
    /// writes a message.
    pub(crate) fn completion(
        &self,
        text: &str,
        reasoning: &str,
        reason: &str,
        usage: Value,
        timings: Value,
    ) -> Value {
        let mut message = json!({"role": "assistant", "content": text});
        if !reasoning.is_empty() {
            message["reasoning_content"] = json!(reasoning);
        }
        let mut whole = self.base("chat.completion");
        whole.insert(
            "choices".to_owned(),
            json!([{"finish_reason": reason, "index": 0, "message": message}]),
        );
        whole.insert("usage".to_owned(), usage);
        whole.insert("timings".to_owned(), timings);
        Value::Object(whole)
    }
}

/// `usage`, as `usage_json_oaicompat` writes it.
pub(crate) fn usage(prompt: u32, cached: u32, completion: u32) -> Value {
    json!({
        "completion_tokens": completion,
        "prompt_tokens": prompt,
        "total_tokens": u64::from(prompt) + u64::from(completion),
        "prompt_tokens_details": {"cached_tokens": cached},
    })
}

/// When a generation's phases ended, for its `timings`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Clock {
    started: Instant,
    prompt_read: Option<Instant>,
    last_token: Option<Instant>,
}

impl Clock {
    /// A clock started now, as the prompt begins to be read.
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            prompt_read: None,
            last_token: None,
        }
    }

    /// Milliseconds since the prompt began to be read.
    pub(crate) fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// A token was sampled now; the first ends the prompt's reading.
    pub(crate) fn token(&mut self) {
        let now = Instant::now();
        self.prompt_read.get_or_insert(now);
        self.last_token = Some(now);
    }

    /// `timings`, as `server_slot_stats::to_json` derives them: a rate
    /// whose divisor is not known yet is 0, and the first token, which the
    /// prompt's last batch yields, is not counted as a generation step.
    pub(crate) fn timings(&self, prompt: u32, cached: u32, generated: u32) -> Value {
        let prompt_n = prompt.saturating_sub(cached);
        let prompt_ms = self
            .prompt_read
            .map_or(0.0, |t| ms(t.duration_since(self.started)));
        let gen_ms = match (self.prompt_read, self.last_token) {
            (Some(read), Some(last)) => ms(last.duration_since(read)).max(0.001),
            _ => 0.0,
        };
        let gen_steps = generated.saturating_sub(1);
        let per = |n: u32, t: f64| if n > 0 { t / f64::from(n) } else { 0.0 };
        let rate = |n: u32, t: f64| if t > 0.0 { 1e3 / t * f64::from(n) } else { 0.0 };
        json!({
            "cache_n": cached,
            "prompt_n": prompt_n,
            "prompt_ms": prompt_ms,
            "prompt_per_token_ms": per(prompt_n, prompt_ms),
            "prompt_per_second": rate(prompt_n, prompt_ms),
            "predicted_n": generated,
            "predicted_ms": gen_ms,
            "predicted_per_token_ms": per(gen_steps, gen_ms),
            "predicted_per_second": rate(gen_steps, gen_ms),
        })
    }
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// llama-server's error kinds, as `format_error_response` names them.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ErrorKind {
    InvalidRequest,
    NotFound,
    Server,
    NotSupported,
    Unavailable,
}

impl ErrorKind {
    fn status_and_type(self) -> (StatusCode, &'static str) {
        match self {
            Self::InvalidRequest => (StatusCode::BAD_REQUEST, "invalid_request_error"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found_error"),
            Self::Server => (StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
            Self::NotSupported => (StatusCode::NOT_IMPLEMENTED, "not_supported_error"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable_error"),
        }
    }
}

/// The `error` object for `kind`, as `format_error_response` writes it.
pub(crate) fn error_body(kind: ErrorKind, message: &str) -> Value {
    let (status, kind) = kind.status_and_type();
    json!({"code": status.as_u16(), "message": message, "type": kind})
}

/// An error answered in place of a reply, `{"error": …}` under its status.
pub(crate) fn error(kind: ErrorKind, message: &str) -> Response {
    let (status, _) = kind.status_and_type();
    (status, Json(json!({"error": error_body(kind, message)}))).into_response()
}

/// The answer to a prompt of `n_prompt` tokens that leaves none of `n_ctx`
/// free: 400 `exceed_context_size_error`, with both counts.
pub(crate) fn over_context(n_prompt: u32, n_ctx: u32) -> Response {
    let message = format!(
        "request ({n_prompt} tokens) exceeds the available context size ({n_ctx} tokens), try increasing it"
    );
    let body = json!({"error": {
        "code": 400,
        "message": message,
        "type": "exceed_context_size_error",
        "n_prompt_tokens": n_prompt,
        "n_ctx": n_ctx,
    }});
    (StatusCode::BAD_REQUEST, Json(body)).into_response()
}
