//! A daemon turn on this machine's model waits for its generation turn
//! behind an image render, and the person watching it is told why.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gglib_core::domain::agent::{AgentEvent, WaitingFor};
use gglib_core::ports::{AdmissionLease, AdmissionRelease};

use crate::handlers::agent::run_fixture::state;
use crate::handlers::agent::turn_fixture::{REPLY, model, phone, turn};

/// The lease a render would hold; nothing to release.
#[derive(Debug)]
struct NoRelease;

impl AdmissionRelease for NoRelease {
    fn release(&self, _slot: usize) {}

    fn progress(&self, _slot: usize) {}
}

/// How long the render holds the GPU in this test.
const RENDER: Duration = Duration::from_millis(600);

/// A paired device's turn, run as `prepare` composes it, while a render
/// holds the daemon's gate: its loop sends `waiting` frames for the image
/// render before anything else, and is served once the render ends.
#[tokio::test]
async fn a_device_turn_waits_out_a_render_and_says_so() {
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;
    let chat = phone(&state, id).await;

    let lease = AdmissionLease::new(Arc::new(NoRelease), 1);
    let render = state
        .generation_gate
        .render_turn(lease, None)
        .await
        .expect("a lone render has its turn");
    let started = Instant::now();
    tokio::spawn(async move {
        tokio::time::sleep(RENDER).await;
        drop(render);
    });

    let done = turn(&state, id, chat, REPLY).await;

    assert!(done.ended_well, "{:?}", done.events);
    assert!(started.elapsed() >= RENDER, "served while the render held");
    assert_eq!(done.requests.len(), 1);
    let first = done.events.first();
    assert!(
        matches!(
            first,
            Some(AgentEvent::Waiting {
                reason: WaitingFor::ImageRender,
                position: 1,
                ..
            })
        ),
        "{:?}",
        done.events
    );
}
