//! `POST /v1/chat/completions`: a chat rendered, decoded on a thread of its
//! own, and answered as llama-server answers it.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tracing::debug;

use super::frames::{self, ErrorKind};
use super::reasoning::{self, ReasoningSplit};
use super::reply::{Asked, Event, Out, Reply, kind_of, message_of};
use super::stop::StopScanner;
use crate::decoder::{Abort, DecoderError, Sampling};
use crate::engine::{Engine, RunError};

/// How many events the decode thread may run ahead of the client.
const EVENTS_AHEAD: usize = 16;

/// Request keys asking for what the engine does not do, refused with a 400
/// rather than ignored: a constrained or scored reply, or a sampler chain
/// of the client's own.
const REFUSED: [&str; 5] = [
    "grammar",
    "json_schema",
    "logit_bias",
    "samplers",
    "dry_sequence_breakers",
];

/// Raises the generation's abort when the reply is dropped: the client went
/// away, or the reply ended.
struct AbortOnDrop(Abort);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.raise();
    }
}

pub(crate) async fn chat_completions(State(engine): State<Arc<Engine>>, body: Bytes) -> Response {
    let Ok(body) = serde_json::from_slice::<Value>(&body) else {
        return frames::error(ErrorKind::InvalidRequest, "the request body is not JSON");
    };
    if let Some(why) = refused(&body) {
        return frames::error(ErrorKind::InvalidRequest, &why);
    }
    let Some(resident) = engine.resident() else {
        return frames::error(ErrorKind::Unavailable, "Loading model");
    };
    let (prompt, start) = match reasoning::rendered(&body, &resident.template) {
        Ok(rendered) => rendered,
        Err(why) => return frames::error(ErrorKind::InvalidRequest, &why),
    };
    let sampling = Sampling::from_request(&body);
    let streamed = body.get("stream").and_then(Value::as_bool) == Some(true);
    let asked = Asked {
        include_usage: body
            .pointer("/stream_options/include_usage")
            .and_then(Value::as_bool)
            == Some(true),
        return_progress: body.get("return_progress").and_then(Value::as_bool) == Some(true),
    };
    let Ok(permit) = Arc::clone(&engine.turn).acquire_owned().await else {
        return frames::error(ErrorKind::Unavailable, "the engine is shutting down");
    };
    let abort = Abort::new();
    let guard = AbortOnDrop(abort.clone());
    let (tx, mut rx) = mpsc::channel(EVENTS_AHEAD);
    let job = Job {
        engine: Arc::clone(&engine),
        resident: resident.id,
        prompt,
        sampling,
        abort: abort.clone(),
    };
    if let Err(e) = job.spawn(tx, permit) {
        return frames::error(ErrorKind::Server, &format!("no thread to decode on: {e}"));
    }
    // As llama-server: a request whose first result is an error is answered
    // with that error alone, streamed or not.
    let first = match rx.recv().await {
        None => {
            let e = thread_lost();
            return frames::error(kind_of(&e), &message_of(&e));
        }
        Some(Event::Done(Err(RunError::OverContext { n_prompt, n_ctx }))) => {
            return frames::over_context(n_prompt, n_ctx);
        }
        Some(Event::Done(Err(e))) => return frames::error(kind_of(&e), &message_of(&e)),
        Some(first) => first,
    };
    let reply = Reply::new(
        &resident.name,
        asked,
        StopScanner::from_request(&body),
        ReasoningSplit::new(start),
        abort,
    );
    if streamed {
        stream(reply, first, rx, guard)
    } else {
        whole(reply, first, rx, guard).await
    }
}

/// Why `body` is refused, if it is.
fn refused(body: &Value) -> Option<String> {
    let present = |key: &str| match body.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(_) => true,
    };
    let structured = body
        .pointer("/response_format/type")
        .and_then(Value::as_str)
        .is_some_and(|t| t != "text");
    let scored = body.get("logprobs").and_then(Value::as_bool) == Some(true)
        || body
            .get("n_probs")
            .and_then(Value::as_u64)
            .is_some_and(|n| n > 0);
    REFUSED
        .iter()
        .find(|key| present(key))
        .map(|key| format!("'{key}' is not supported by gglib's in-process engine"))
        .or_else(|| {
            structured.then(|| {
                "a structured 'response_format' is not supported by gglib's in-process engine"
                    .to_owned()
            })
        })
        .or_else(|| {
            scored.then(|| {
                "log probabilities are not supported by gglib's in-process engine".to_owned()
            })
        })
}

/// One generation, sent to a thread of its own.
struct Job {
    engine: Arc<Engine>,
    resident: u64,
    prompt: String,
    sampling: Sampling,
    abort: Abort,
}

impl Job {
    /// Run on a new thread, holding `permit` until done, sending its events
    /// to `tx`. A send that fails means the reply is gone, and raises the
    /// abort.
    fn spawn(self, tx: mpsc::Sender<Event>, permit: OwnedSemaphorePermit) -> std::io::Result<()> {
        let thread = std::thread::Builder::new().name("gglib-decode".to_owned());
        thread.spawn(move || {
            let _permit = permit;
            let Self {
                engine,
                resident,
                prompt,
                sampling,
                abort,
            } = self;
            let result = engine.run(resident, &prompt, &sampling, &abort, &mut |progress| {
                if tx.blocking_send(Event::Progress(progress)).is_err() {
                    abort.raise();
                }
            });
            if tx.blocking_send(Event::Done(result)).is_err() {
                debug!("the reply was gone before its generation ended");
            }
        })?;
        Ok(())
    }
}

/// How a generation ended whose thread stopped without saying: it panicked.
fn thread_lost() -> RunError {
    let why = "the decode thread stopped without an answer".to_owned();
    RunError::Decoder(DecoderError::Decode(why))
}

/// [`thread_lost`], as the event the thread did not send.
fn unanswered() -> Event {
    Event::Done(Err(thread_lost()))
}

/// What the streamed body has yet to send.
struct Streaming {
    reply: Reply,
    first: Option<Event>,
    rx: mpsc::Receiver<Event>,
    queued: VecDeque<String>,
    ended: bool,
    _abort: AbortOnDrop,
}

fn frame(chunk: &Value) -> String {
    format!("data: {chunk}\n\n")
}

/// The reply as `text/event-stream`: chunks as they come, `[DONE]` at the
/// end, or an error frame and no `[DONE]` when the generation fails.
fn stream(reply: Reply, first: Event, rx: mpsc::Receiver<Event>, abort: AbortOnDrop) -> Response {
    let state = Streaming {
        reply,
        first: Some(first),
        rx,
        queued: VecDeque::new(),
        ended: false,
        _abort: abort,
    };
    let body = futures_util::stream::unfold(state, |mut s| async move {
        loop {
            if let Some(next) = s.queued.pop_front() {
                return Some((Ok::<_, Infallible>(next), s));
            }
            if s.ended {
                return None;
            }
            let event = match s.first.take() {
                Some(event) => event,
                None => s.rx.recv().await.unwrap_or_else(unanswered),
            };
            match s.reply.take(event) {
                Out::Chunks(chunks) => s.queued.extend(chunks.iter().map(frame)),
                Out::End(chunks) => {
                    s.queued.extend(chunks.iter().map(frame));
                    s.queued.push_back("data: [DONE]\n\n".to_owned());
                    s.ended = true;
                }
                Out::Failed(kind, message) => {
                    let error = json!({"error": frames::error_body(kind, &message)});
                    s.queued.push_back(frame(&error));
                    s.ended = true;
                }
            }
        }
    });
    (
        [(CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(body),
    )
        .into_response()
}

/// The reply as one `chat.completion`, once the generation has ended.
async fn whole(
    mut reply: Reply,
    first: Event,
    mut rx: mpsc::Receiver<Event>,
    _abort: AbortOnDrop,
) -> Response {
    let mut event = first;
    loop {
        let finished = match &event {
            Event::Done(Ok(finished)) => Some(*finished),
            _ => None,
        };
        match (reply.take(event), finished) {
            (Out::End(_), Some(finished)) => {
                return Json(reply.completion(&finished)).into_response();
            }
            (Out::Failed(kind, message), _) => return frames::error(kind, &message),
            _ => {}
        }
        event = rx.recv().await.unwrap_or_else(unanswered);
    }
}
