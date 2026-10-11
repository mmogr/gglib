//! `GET /api/generation/turn` against the daemon's real gate: what the
//! stream says, and that the turn lasts exactly as long as the connection.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::Router;
use axum::response::IntoResponse as _;
use axum::routing::get;
use futures_util::StreamExt as _;
use gglib_core::ports::{AdmissionLease, AdmissionRelease, GateError, GenerationTurn};
use gglib_core::sse::{DataFrames, Event as SseEvent};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::handlers::agent::run_fixture::state;

/// The lease a render would hold; nothing to release.
#[derive(Debug)]
struct NoRelease;

impl AdmissionRelease for NoRelease {
    fn release(&self, _slot: usize) {}

    fn progress(&self, _slot: usize) {}
}

fn lease() -> AdmissionLease {
    AdmissionLease::new(Arc::new(NoRelease), 1)
}

/// Serve the route on a loopback port over `state`; its URL.
async fn serve(state: AppState) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/turn", listener.local_addr().unwrap());
    let app = Router::new().route("/turn", get(turn)).with_state(state);
    tokio::spawn(async move { axum::serve(listener, app).await });
    url
}

/// Read `body` until an event named `name` arrives; every event read, that
/// one last.
async fn until<S, B, E>(body: &mut S, frames: &mut DataFrames, name: &str) -> Vec<SseEvent>
where
    S: futures_util::Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Debug,
{
    let mut read = Vec::new();
    let found = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(chunk) = body.next().await {
            for event in frames.push_events(chunk.unwrap().as_ref()) {
                let done = event.name.as_deref() == Some(name);
                read.push(event);
                if done {
                    return;
                }
            }
        }
    })
    .await;
    assert!(found.is_ok(), "no `{name}` in {read:?}");
    read
}

fn data(event: &SseEvent) -> Value {
    serde_json::from_str(&event.data).unwrap()
}

/// Behind a held render the route says `waiting`, with the render's step
/// and its place in line, and `granted` once the render ends.
#[tokio::test]
async fn granted_follows_waiting() {
    let (_dir, state) = state().await;
    let render = state.generation_gate.render_turn(lease(), None).await;
    let render = render.expect("a lone render has its turn");
    let url = serve(Arc::clone(&state)).await;

    let response = reqwest::get(&url).await.unwrap();
    assert!(response.status().is_success());
    let mut body = response.bytes_stream();
    let mut frames = DataFrames::unbounded();
    let first = until(&mut body, &mut frames, GENERATION_TURN_WAITING).await;
    assert_eq!(
        data(first.last().unwrap()),
        json!({ "step": 0, "total": 0, "position": 1 })
    );

    drop(render);
    let rest = until(&mut body, &mut frames, GENERATION_TURN_GRANTED).await;
    let names: Vec<_> = rest.iter().map(|e| e.name.as_deref()).collect();
    assert!(
        names
            .iter()
            .all(|n| [Some(GENERATION_TURN_WAITING), Some(GENERATION_TURN_GRANTED)].contains(n)),
        "{names:?}"
    );
}

/// A connection holding its turn keeps a render waiting; once the client
/// goes away, the render has its turn within a keep-alive or two.
#[tokio::test]
async fn a_held_connection_blocks_a_render_until_the_client_goes() {
    let (_dir, state) = state().await;
    let url = serve(Arc::clone(&state)).await;

    let response = reqwest::get(&url).await.unwrap();
    let mut body = response.bytes_stream();
    let mut frames = DataFrames::unbounded();
    until(&mut body, &mut frames, GENERATION_TURN_GRANTED).await;

    let gate = Arc::clone(&state.generation_gate);
    let blocked = tokio::time::timeout(Duration::from_millis(500), gate.render_turn(lease(), None));
    assert!(blocked.await.is_err(), "a render ran beside a held turn");

    drop(body);
    let gone = Instant::now();
    let freed = tokio::time::timeout(KEEP_ALIVE * 2 + Duration::from_secs(2), async {
        gate.render_turn(lease(), None).await
    })
    .await;
    let render = freed.expect("the turn outlived its connection");
    assert!(render.is_ok(), "{render:?}");
    assert!(gone.elapsed() <= KEEP_ALIVE * 2 + Duration::from_secs(2));
}

/// A gate that has stalled.
#[derive(Debug)]
struct Stalled;

#[async_trait]
impl GenerationGate for Stalled {
    async fn llm_turn(
        &self,
        _observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        Err(GateError::Stalled(Duration::from_mins(3)))
    }

    async fn render_turn(
        &self,
        _lease: AdmissionLease,
        _observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        unreachable!("the route asks for no render turn")
    }
}

/// The events a stream gives before it ends.
async fn to_end(
    events: impl Stream<Item = Result<Event, Infallible>> + Send + 'static,
) -> Vec<SseEvent> {
    let body = Sse::new(events).into_response().into_body();
    let bytes = body.collect().await.unwrap().to_bytes();
    DataFrames::unbounded().push_events(&bytes)
}

/// A gate that grants no turn is answered `refused`, with why and how long
/// it waited, and the stream ends.
#[tokio::test]
async fn a_refused_turn_says_why_and_ends() {
    let events = to_end(turn_events(Arc::new(Stalled), None)).await;
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].name.as_deref(), Some(GENERATION_TURN_REFUSED));
    let refusal = data(&events[0]);
    assert_eq!(refusal["stalled_secs"], json!(180));
    assert!(refusal["message"].as_str().unwrap().contains("180s"));
}

/// A held turn's stream ends at the daemon's shutdown and gives the turn
/// back, so it never holds the daemon open.
#[tokio::test]
async fn a_held_turn_ends_at_shutdown() {
    let (_dir, state) = state().await;
    let shutdown = CancellationToken::new();
    let events = turn_events(Arc::clone(&state.generation_gate), Some(shutdown.clone()));
    let read = tokio::spawn(to_end(events));
    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown.cancel();

    let events = tokio::time::timeout(Duration::from_secs(5), read).await;
    let events = events.expect("the stream ends").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name.as_deref(), Some(GENERATION_TURN_GRANTED));
    let render = tokio::time::timeout(
        Duration::from_secs(1),
        state.generation_gate.render_turn(lease(), None),
    );
    assert!(render.await.is_ok_and(|turn| turn.is_ok()));
}
