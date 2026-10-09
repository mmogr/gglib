//! The generation gate, driven through the real [`AdmissionQueue`].
//!
//! Each wait is a future polled by hand with a no-op waker, so a test says
//! exactly when a waiter asks again; the clock is paused where a deadline is
//! in play.

use std::future::Future;
use std::pin::{Pin, pin};
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use gglib_core::domain::{CacheRamHealth, RuntimeKind, SecondarySlotDecision};
use gglib_core::ports::{GateWait, WaitReason};

use super::super::{ADMISSION_DEADLINE, AdmissionDecision, Candidate, PRIMARY_SLOT, Ticket};
use super::*;

const NEVER_FITS: SecondarySlotDecision = SecondarySlotDecision::RefuseTooLarge {
    footprint_bytes: 9 << 30,
    ceiling_bytes: 2 << 30,
};
const ALWAYS_FITS: SecondarySlotDecision = SecondarySlotDecision::Grant {
    footprint_bytes: 1 << 28,
    headroom_bytes: 8 << 30,
};

/// The chat model's id and the image model's, as the residents carry them.
const QWEN: u32 = 1;
const FLUX: u32 = 9;

fn resident(model_id: u32, name: &str, runtime: RuntimeKind) -> Resident {
    Resident {
        model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
        model_id,
        model_name: name.to_string(),
        context_size: 4096,
        port: 8000 + u16::try_from(model_id).unwrap_or(0),
        projector: None,
        runtime,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 512 * 1024 * 1024,
    }
}

/// A queue with qwen idle in the primary and flux in the second slot, and
/// the lease flux's launch left behind: the render's own `sd-server` lease.
fn chat_and_image() -> (Arc<AdmissionQueue>, AdmissionLease) {
    let q = Arc::new(AdmissionQueue::new());
    let ticket = q.enqueue("qwen");
    assert!(matches!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Launch { slot: 0, .. }
    ));
    drop(q.install(PRIMARY_SLOT, resident(QWEN, "qwen", RuntimeKind::Llama)));
    let ticket = q.enqueue("flux");
    let image = Candidate::image(ALWAYS_FITS);
    assert!(matches!(
        q.poll(&ticket, image),
        AdmissionDecision::Launch { slot: 1, .. }
    ));
    let lease = q.install(1, resident(FLUX, "flux", RuntimeKind::StableDiffusion));
    (q, lease)
}

/// Poll a wait once.
fn poll<F: Future>(wait: Pin<&mut F>) -> Poll<F::Output> {
    wait.poll(&mut Context::from_waker(Waker::noop()))
}

/// Poll a wait that must have its turn now.
fn granted<F: Future<Output = Result<GenerationTurn, GateError>>>(
    wait: Pin<&mut F>,
) -> GenerationTurn {
    match poll(wait) {
        Poll::Ready(Ok(turn)) => turn,
        Poll::Ready(Err(e)) => panic!("expected a turn, got {e}"),
        Poll::Pending => panic!("expected a turn, still waiting"),
    }
}

/// A chat request for the resident qwen, polled once.
fn chat(q: &Arc<AdmissionQueue>) -> (Ticket, AdmissionDecision) {
    let ticket = q.enqueue("qwen");
    let decision = q.poll(&ticket, NEVER_FITS);
    (ticket, decision)
}

fn inflight(q: &AdmissionQueue, slot: usize) -> u32 {
    q.slot(slot).map_or(0, |r| r.inflight)
}

/// Rule 1: a render on a resident image model is not held back by its own
/// lease.
#[tokio::test]
async fn a_lone_render_gets_its_turn_at_once() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    assert_eq!(turn.kind(), TurnKind::Render);
    assert_eq!(inflight(&q, 1), 1, "the turn holds the render's lease");
}

/// Rule 1: the fast path ignores the gate for an `sd-server` resident, so
/// one with room is served while another renders.
#[tokio::test]
async fn an_image_model_is_served_while_a_render_holds_the_gate() {
    let q = Arc::new(AdmissionQueue::new());
    // Two image models: sdxl swaps qwen out of the primary, flux loads beside.
    let ticket = q.enqueue("qwen");
    assert!(matches!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Launch { slot: 0, .. }
    ));
    drop(q.install(PRIMARY_SLOT, resident(QWEN, "qwen", RuntimeKind::Llama)));
    let ticket = q.enqueue("sdxl");
    let no_room = Candidate::image(SecondarySlotDecision::RefuseUnknownBudget);
    assert!(matches!(
        q.poll(&ticket, no_room),
        AdmissionDecision::Launch { slot: 0, .. }
    ));
    let lease = q.install(0, resident(5, "sdxl", RuntimeKind::StableDiffusion));
    let ticket = q.enqueue("flux");
    assert!(matches!(
        q.poll(&ticket, Candidate::image(ALWAYS_FITS)),
        AdmissionDecision::Launch { slot: 1, .. }
    ));
    drop(q.install(1, resident(FLUX, "flux", RuntimeKind::StableDiffusion)));

    let gate = q.generation_gate();
    let _turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let ticket = q.enqueue("flux");
    assert_eq!(
        q.poll(&ticket, Candidate::image(ALWAYS_FITS)),
        AdmissionDecision::Serve { slot: 1 }
    );
    drop(q.claim(1));
}

/// Rule 3: a render waits for a chat in flight; chats after it wait, on the
/// fast path and as turns, and are served once it ends. So does a launch.
#[tokio::test]
async fn a_render_waits_for_chats_in_flight_and_chats_queue_behind_it() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let in_flight = q.lease(PRIMARY_SLOT).expect("qwen is resident");

    let mut render = pin!(gate.render_turn(lease, None));
    assert!(poll(render.as_mut()).is_pending(), "a chat is in flight");

    let (later, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait, "the fast path waits");
    let mut llm = pin!(gate.llm_turn(None));
    assert!(poll(llm.as_mut()).is_pending(), "a turn waits");

    drop(in_flight);
    let turn = granted(render.as_mut());
    assert_eq!(q.poll(&later, NEVER_FITS), AdmissionDecision::Wait);
    assert!(poll(llm.as_mut()).is_pending());

    drop(turn);
    assert_eq!(
        q.poll(&later, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT }
    );
    drop(q.claim(PRIMARY_SLOT));
    drop(granted(llm.as_mut()));
}

/// Rule 3: a llama-server launch is generation too, so it waits while a
/// render holds the GPU and goes when it ends.
#[tokio::test]
async fn a_chat_launch_waits_for_a_held_render() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());

    let cold = q.enqueue("llama-70b");
    assert_eq!(q.poll(&cold, NEVER_FITS), AdmissionDecision::Wait);
    drop(turn);
    assert_eq!(
        q.poll(&cold, NEVER_FITS),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: Some(QWEN)
        }
    );
}

/// Rule 3, order: render, chat, render, chat are granted in that order,
/// whichever asks first after each turn ends.
#[tokio::test]
async fn turns_are_first_come_first_served() {
    let (q, lease) = chat_and_image();
    let second_lease = q.lease(1).expect("flux is resident");
    let gate = q.generation_gate();

    let r1 = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let mut c1 = pin!(gate.llm_turn(None));
    let mut r2 = pin!(gate.render_turn(second_lease, None));
    let mut c2 = pin!(gate.llm_turn(None));
    assert!(poll(c1.as_mut()).is_pending());
    assert!(poll(r2.as_mut()).is_pending());
    assert!(poll(c2.as_mut()).is_pending());

    drop(r1);
    assert!(poll(r2.as_mut()).is_pending(), "the older chat goes first");
    assert!(poll(c2.as_mut()).is_pending(), "behind the waiting render");
    let c1 = granted(c1.as_mut());

    assert!(poll(c2.as_mut()).is_pending());
    assert!(poll(r2.as_mut()).is_pending(), "a chat is in flight");
    drop(c1);
    assert!(poll(c2.as_mut()).is_pending(), "the render is older");
    let r2 = granted(r2.as_mut());
    assert!(poll(c2.as_mut()).is_pending());
    drop(r2);
    drop(granted(c2.as_mut()));
}

/// Rule 3, starvation: a stream of chats arriving after a waiting render
/// all wait, so the render goes as soon as the chat before it ends.
#[tokio::test]
async fn back_to_back_chats_cannot_starve_a_waiting_render() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let in_flight = q.lease(PRIMARY_SLOT).expect("qwen is resident");
    let mut render = pin!(gate.render_turn(lease, None));
    assert!(poll(render.as_mut()).is_pending());

    let mut later = Vec::new();
    for _ in 0..5 {
        let (ticket, decision) = chat(&q);
        assert_eq!(decision, AdmissionDecision::Wait);
        later.push(ticket);
    }
    drop(in_flight);
    for ticket in &later {
        assert_eq!(q.poll(ticket, NEVER_FITS), AdmissionDecision::Wait);
    }
    drop(granted(render.as_mut()));
}

/// Rule 3, one render at a time: a second render, with nothing else ahead
/// of it, waits while the first holds the GPU and goes when it ends.
#[tokio::test]
async fn a_second_render_waits_while_one_is_held() {
    let (q, lease) = chat_and_image();
    let second_lease = q.lease(1).expect("flux is resident");
    let gate = q.generation_gate();

    let first = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let mut second = pin!(gate.render_turn(second_lease, None));
    assert!(poll(second.as_mut()).is_pending(), "a render holds the GPU");

    drop(first);
    drop(granted(second.as_mut()));
}

/// A render whose caller went away leaves the line: a dropped wait, queued
/// behind a chat in flight, holds back neither a new turn nor the fast path.
#[tokio::test]
async fn a_dropped_render_wait_leaves_the_line() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let in_flight = q.lease(PRIMARY_SLOT).expect("qwen is resident");

    {
        let mut render = pin!(gate.render_turn(lease, None));
        assert!(poll(render.as_mut()).is_pending(), "a chat is in flight");
        let (_, decision) = chat(&q);
        assert_eq!(decision, AdmissionDecision::Wait, "behind the render");
    }

    drop(granted(pin!(gate.llm_turn(None)).as_mut()));
    drop(in_flight);
    let (_ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Serve { slot: PRIMARY_SLOT });
    drop(q.claim(PRIMARY_SLOT));
}

/// The #722 shape: an older chat waits to evict the very slot the waiting
/// render's lease pins (qwen is held by a run, so the second slot is the
/// only way in). Tickets are not turns, and a hold is not one either, so the
/// render goes and the chat follows it.
#[tokio::test]
async fn an_older_chat_waiting_on_the_render_slot_does_not_deadlock_it() {
    let (q, lease) = chat_and_image();
    let _hold = q.hold(8001, QWEN).expect("qwen is resident on its port");
    let older = q.enqueue("embedder");
    assert_eq!(q.poll(&older, ALWAYS_FITS), AdmissionDecision::Wait);

    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    assert_eq!(q.poll(&older, ALWAYS_FITS), AdmissionDecision::Wait);

    drop(turn);
    assert_eq!(
        q.poll(&older, ALWAYS_FITS),
        AdmissionDecision::Launch {
            slot: 1,
            evict: Some(FLUX)
        }
    );
}

/// Rule 4: render steps are progress, so a chat waiting behind a render, as
/// a ticket and as a turn, outlives the deadline while steps arrive; with
/// none for the whole deadline both give up.
#[tokio::test]
async fn steps_keep_waiters_alive_and_silence_expires_them() {
    tokio::time::pause();
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());

    let (ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);
    let mut llm = pin!(gate.llm_turn(None));
    assert!(poll(llm.as_mut()).is_pending());

    for step in 1..=3 {
        tokio::time::advance(ADMISSION_DEADLINE * 2 / 3).await;
        turn.progress(step, 20);
        assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Wait);
        assert!(poll(llm.as_mut()).is_pending(), "step {step}");
    }

    tokio::time::advance(ADMISSION_DEADLINE).await;
    assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Expired);
    assert!(matches!(
        poll(llm.as_mut()),
        Poll::Ready(Err(GateError::Stalled(_)))
    ));
}

/// A turn ending is queue progress: a chat still waiting after it, now
/// behind the render that went next, starts its stall clock again.
#[tokio::test]
async fn a_turn_ending_restarts_the_stall_clock() {
    tokio::time::pause();
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let llm = granted(pin!(gate.llm_turn(None)).as_mut());
    let mut render = pin!(gate.render_turn(lease, None));
    assert!(poll(render.as_mut()).is_pending());
    let (ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);

    tokio::time::advance(ADMISSION_DEADLINE * 5 / 6).await;
    drop(llm);
    let _render = granted(render.as_mut());
    assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Wait);
    tokio::time::advance(ADMISSION_DEADLINE / 3).await;
    assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Wait);
}

/// Rule 7: a render turn moved into a task keeps its lease, and the model's
/// count, until the task drops it.
#[tokio::test]
async fn a_turn_moved_into_a_task_keeps_its_lease_until_it_ends() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let (finish, finished) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _ = finished.await;
        drop(turn);
    });

    tokio::task::yield_now().await;
    assert_eq!(inflight(&q, 1), 1);
    assert!(poll(pin!(gate.llm_turn(None)).as_mut()).is_pending());

    finish.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(inflight(&q, 1), 0);
    drop(granted(pin!(gate.llm_turn(None)).as_mut()));
}

/// Rule 7: the process is killed while the render still holds its turn and
/// its count; then the slot is emptied; then the turn ends.
#[tokio::test]
async fn retiring_a_render_kills_then_releases_and_evicts_then_ends() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());

    let at_kill = Mutex::new(None);
    let retired = q
        .retire_render(turn, FLUX, async {
            let held = poll(pin!(gate.llm_turn(None)).as_mut()).is_pending();
            *at_kill.lock().unwrap() = Some((q.slot(1).map(|r| r.inflight), held));
        })
        .await;

    assert_eq!(
        *at_kill.lock().unwrap(),
        Some((Some(1), true)),
        "at the kill the render still holds its count and its turn"
    );
    assert_eq!(retired.map(|r| r.model_id), Some(FLUX));
    assert!(q.slot(1).is_none(), "the slot is emptied");
    drop(granted(pin!(gate.llm_turn(None)).as_mut()));
}

/// Rule 7: when the slot was taken by a newcomer while the process died,
/// retiring leaves the newcomer and its request alone.
#[tokio::test]
async fn retiring_a_render_spares_a_newcomer_in_its_slot() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());

    let newcomer = Mutex::new(None);
    let retired = q
        .retire_render(turn, FLUX, async {
            // An explicit stop empties the slot and another model lands there
            // with a request in flight.
            assert!(q.evict(1).is_some());
            *newcomer.lock().unwrap() =
                Some(q.install(1, resident(7, "embedder", RuntimeKind::Llama)));
        })
        .await;

    assert!(
        retired.is_none(),
        "nothing of the render's was left to retire"
    );
    assert_eq!(q.slot(1).map(|r| r.model_id), Some(7));
    assert_eq!(inflight(&q, 1), 1, "the newcomer keeps its request");
    drop(newcomer);
}

/// Rule 8: a chat waiting behind a render is told the render's step, total
/// and its own place, and told again only when that changes.
#[tokio::test]
async fn the_observer_sees_the_render_step() {
    #[derive(Debug, Default)]
    struct Seen(Mutex<Vec<GateWait>>);
    impl GateWaitObserver for Seen {
        fn waiting(&self, wait: GateWait) {
            self.0.lock().unwrap().push(wait);
        }
    }

    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let seen = Arc::new(Seen::default());
    let observer = Arc::clone(&seen) as Arc<dyn GateWaitObserver>;
    let mut llm = pin!(gate.llm_turn(Some(observer)));

    assert!(poll(llm.as_mut()).is_pending());
    // Woken with nothing new to say: no second report.
    q.notify();
    assert!(poll(llm.as_mut()).is_pending());
    turn.progress(3, 20);
    assert!(poll(llm.as_mut()).is_pending());

    let wait = |step, total| GateWait {
        reason: WaitReason::ImageRender,
        step,
        total,
        position: 1,
    };
    assert_eq!(*seen.0.lock().unwrap(), [wait(0, 0), wait(3, 20)]);
    drop(turn);
    drop(granted(llm.as_mut()));
}
