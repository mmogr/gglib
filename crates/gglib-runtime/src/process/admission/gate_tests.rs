//! The generation gate, driven through the real [`AdmissionQueue`].
//!
//! Each wait is a future polled by hand with a no-op waker, so a test says
//! exactly when a waiter asks again; the clock is paused where a deadline is
//! in play.

use std::future::Future;
use std::pin::{Pin, pin};
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use gglib_core::domain::{
    CacheRamHealth, GenerationSnapshot, RenderSnapshot, RuntimeKind, SecondarySlotDecision,
};
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
    let ticket = q.enqueue(QWEN, "qwen");
    assert!(matches!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Launch { slot: 0, .. }
    ));
    drop(q.install(PRIMARY_SLOT, resident(QWEN, "qwen", RuntimeKind::Llama)));
    let ticket = q.enqueue(FLUX, "flux");
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
    let ticket = q.enqueue(QWEN, "qwen");
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
    let _turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    assert_eq!(inflight(&q, 1), 1, "the turn holds the render's lease");
}

/// Rule 1: the fast path ignores the gate for an `sd-server` resident, so
/// one with room is served while another renders.
#[tokio::test]
async fn an_image_model_is_served_while_a_render_holds_the_gate() {
    let q = Arc::new(AdmissionQueue::new());
    // Two image models: sdxl swaps qwen out of the primary, and flux is
    // recorded beside it directly. The queue places no second image model
    // beside a first, and with one request at a time for each, only a
    // second one shows the fast path passing the gate.
    let ticket = q.enqueue(QWEN, "qwen");
    assert!(matches!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Launch { slot: 0, .. }
    ));
    drop(q.install(PRIMARY_SLOT, resident(QWEN, "qwen", RuntimeKind::Llama)));
    let ticket = q.enqueue(5, "sdxl");
    let no_room = Candidate::image(SecondarySlotDecision::RefuseUnknownBudget);
    assert!(matches!(
        q.poll(&ticket, no_room),
        AdmissionDecision::Launch { slot: 0, .. }
    ));
    let lease = q.install(0, resident(5, "sdxl", RuntimeKind::StableDiffusion));
    drop(q.install(1, resident(FLUX, "flux", RuntimeKind::StableDiffusion)));

    let gate = q.generation_gate();
    let _turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let ticket = q.enqueue(FLUX, "flux");
    assert_eq!(
        q.poll(&ticket, Candidate::image(ALWAYS_FITS)),
        AdmissionDecision::Serve { slot: 1 }
    );
    drop(q.claim(1));
}

/// Rule 3: a render waits for a chat in flight; chats after it wait on the
/// fast path, and are served once it ends. So does a launch.
#[tokio::test]
async fn a_render_waits_for_chats_in_flight_and_chats_queue_behind_it() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let in_flight = q.lease(PRIMARY_SLOT).expect("qwen is resident");

    let mut render = pin!(gate.render_turn(lease, None));
    assert!(poll(render.as_mut()).is_pending(), "a chat is in flight");

    let (later, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait, "the fast path waits");

    drop(in_flight);
    assert_eq!(
        q.poll(&later, NEVER_FITS),
        AdmissionDecision::Wait,
        "behind the waiting render"
    );
    let turn = granted(render.as_mut());
    assert_eq!(q.poll(&later, NEVER_FITS), AdmissionDecision::Wait);

    drop(turn);
    assert_eq!(
        q.poll(&later, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT }
    );
    drop(q.claim(PRIMARY_SLOT));
}

/// Rule 3: a llama-server launch is generation too, so it waits while a
/// render holds the GPU and goes when it ends.
#[tokio::test]
async fn a_chat_launch_waits_for_a_held_render() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());

    let cold = q.enqueue(3, "llama-70b");
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

/// Rule 3, order: render, chat, render, chat go in that order. When the
/// first render ends the older chat is served, the younger one waits behind
/// the second render, and that render waits for the older chat to end.
#[tokio::test]
async fn turns_are_first_come_first_served() {
    let (q, lease) = chat_and_image();
    let second_lease = q.lease(1).expect("flux is resident");
    let gate = q.generation_gate();

    let r1 = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let (c1, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);
    let mut r2 = pin!(gate.render_turn(second_lease, None));
    assert!(poll(r2.as_mut()).is_pending());
    let (c2, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);

    drop(r1);
    assert_eq!(
        q.poll(&c2, NEVER_FITS),
        AdmissionDecision::Wait,
        "behind the waiting render"
    );
    assert_eq!(
        q.poll(&c1, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT },
        "the older chat goes first"
    );
    let c1 = q.claim(PRIMARY_SLOT);
    assert!(poll(r2.as_mut()).is_pending(), "a chat is in flight");

    drop(c1);
    assert_eq!(
        q.poll(&c2, NEVER_FITS),
        AdmissionDecision::Wait,
        "the render is older"
    );
    let r2 = granted(r2.as_mut());
    assert_eq!(q.poll(&c2, NEVER_FITS), AdmissionDecision::Wait);
    drop(r2);
    assert_eq!(
        q.poll(&c2, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT }
    );
    drop(q.claim(PRIMARY_SLOT));
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
/// behind a chat in flight, does not hold back the chats after it.
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

    drop(in_flight);
    let (_ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Serve { slot: PRIMARY_SLOT });
    drop(q.claim(PRIMARY_SLOT));
}

/// The #722 shape: an older chat waits to evict the very slot the waiting
/// render's lease pins (another chat model is loading into the primary, so
/// the second slot is the only way in). Tickets are not turns, so the render
/// goes and the chat follows it.
#[tokio::test]
async fn an_older_chat_waiting_on_the_render_slot_does_not_deadlock_it() {
    let (q, lease) = chat_and_image();
    assert!(q.evict(PRIMARY_SLOT).is_some());
    let loading = q.enqueue(3, "llama-70b");
    assert_eq!(
        q.poll(&loading, NEVER_FITS),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: None
        }
    );
    let older = q.enqueue(7, "embedder");
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

/// Rule 4: render steps are progress, so a chat waiting behind a render, and
/// a second render waiting for its turn, outlive the deadline while steps
/// arrive; with none for the whole deadline both give up.
#[tokio::test]
async fn steps_keep_waiters_alive_and_silence_expires_them() {
    tokio::time::pause();
    let (q, lease) = chat_and_image();
    let second_lease = q.lease(1).expect("flux is resident");
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());

    let (ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);
    let mut second = pin!(gate.render_turn(second_lease, None));
    assert!(poll(second.as_mut()).is_pending());

    for step in 1..=3 {
        tokio::time::advance(ADMISSION_DEADLINE * 2 / 3).await;
        turn.progress(step, 20);
        assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Wait);
        assert!(poll(second.as_mut()).is_pending(), "step {step}");
    }

    tokio::time::advance(ADMISSION_DEADLINE).await;
    assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Expired);
    assert!(matches!(
        poll(second.as_mut()),
        Poll::Ready(Err(GateError::Stalled(_)))
    ));
}

/// A turn ending is queue progress: a chat still waiting after it, now
/// behind the render that went next, starts its stall clock again. The
/// first render's lease is taken out of its turn and kept, so only the
/// turn's end moves the queue.
#[tokio::test]
async fn a_turn_ending_restarts_the_stall_clock() {
    tokio::time::pause();
    let (q, lease) = chat_and_image();
    let second_lease = q.lease(1).expect("flux is resident");
    let gate = q.generation_gate();
    let mut first = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let _kept = first.take_lease().expect("a render turn owns its lease");
    let mut second = pin!(gate.render_turn(second_lease, None));
    assert!(poll(second.as_mut()).is_pending());
    let (ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);

    tokio::time::advance(ADMISSION_DEADLINE * 5 / 6).await;
    drop(first);
    let _second = granted(second.as_mut());
    assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Wait);
    tokio::time::advance(ADMISSION_DEADLINE / 3).await;
    assert_eq!(q.poll(&ticket, NEVER_FITS), AdmissionDecision::Wait);
}

/// Rule 7: a render turn moved into a task keeps its lease, and the model's
/// count, until the task drops it; a chat waits behind it until then.
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
    let (ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);

    finish.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(inflight(&q, 1), 0);
    assert_eq!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT }
    );
    drop(q.claim(PRIMARY_SLOT));
}

/// Rule 7: the process is killed while the render still holds its turn and
/// its count; then the slot is emptied; then the turn ends.
#[tokio::test]
async fn retiring_a_render_kills_then_releases_and_evicts_then_ends() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let (ticket, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);

    let at_kill = Mutex::new(None);
    let retired = q
        .retire_render(turn, FLUX, async {
            let held = q.poll(&ticket, NEVER_FITS) == AdmissionDecision::Wait;
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
    assert_eq!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT }
    );
    drop(q.claim(PRIMARY_SLOT));
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

/// Rule 8: a render waiting behind another is told that render's step and
/// total and its own place, and told again only when that changes.
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
    let second_lease = q.lease(1).expect("flux is resident");
    let gate = q.generation_gate();
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    let seen = Arc::new(Seen::default());
    let observer = Arc::clone(&seen) as Arc<dyn GateWaitObserver>;
    let mut second = pin!(gate.render_turn(second_lease, Some(observer)));

    assert!(poll(second.as_mut()).is_pending());
    // Woken with nothing new to say: no second report.
    q.notify();
    assert!(poll(second.as_mut()).is_pending());
    turn.progress(3, 20);
    assert!(poll(second.as_mut()).is_pending());

    let wait = |step, total| GateWait {
        reason: WaitReason::ImageRender,
        step,
        total,
        position: 1,
    };
    assert_eq!(*seen.0.lock().unwrap(), [wait(0, 0), wait(3, 20)]);
    drop(turn);
    drop(granted(second.as_mut()));
}

/// The dashboard's view of the gate: the render holding it names its image
/// model and step, the chats in flight are counted, and so are the renders
/// waiting.
#[tokio::test]
async fn the_snapshot_shows_the_render_the_chats_and_the_line() {
    let (q, lease) = chat_and_image();
    let second_lease = q.lease(1).expect("flux is resident");
    let gate = q.generation_gate();
    assert_eq!(q.snapshot().generation, GenerationSnapshot::default());

    let in_flight = q.lease(PRIMARY_SLOT).expect("qwen is resident");
    assert_eq!(
        q.snapshot().generation.llm_inflight,
        1,
        "a chat lease counts"
    );
    let mut render = pin!(gate.render_turn(lease, None));
    assert!(poll(render.as_mut()).is_pending());
    let generation = q.snapshot().generation;
    assert_eq!((generation.render, generation.waiting), (None, 1));

    drop(in_flight);
    let turn = granted(render.as_mut());
    turn.progress(2, 4);
    let mut second = pin!(gate.render_turn(second_lease, None));
    assert!(poll(second.as_mut()).is_pending());
    assert_eq!(
        q.snapshot().generation,
        GenerationSnapshot {
            render: Some(RenderSnapshot {
                model_name: Some("flux".to_owned()),
                step: 2,
                total: 4,
            }),
            llm_inflight: 0,
            waiting: 1,
        }
    );

    drop(turn);
    drop(granted(second.as_mut()));
}

/// A Stop on the image model asks the render that draws with it and empties
/// nothing: the slot and the render's count stand until the render is
/// retired, and only the render it was asked of reads the question.
#[tokio::test]
async fn a_stop_asks_the_render_that_holds_the_model() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    assert!(!q.ask_render_stop(FLUX), "a lease alone is no render");
    let turn = granted(pin!(gate.render_turn(lease, None)).as_mut());
    assert!(!q.render_stop_asked(FLUX));

    assert!(!q.ask_render_stop(QWEN), "the chat model draws nothing");
    assert!(!q.render_stop_asked(FLUX));
    assert!(q.ask_render_stop(FLUX));

    assert!(q.render_stop_asked(FLUX));
    assert!(!q.render_stop_asked(QWEN));
    assert_eq!(inflight(&q, 1), 1, "nothing was emptied or released");
    let retired = q.retire_render(turn, FLUX, async {}).await;
    assert_eq!(retired.map(|r| r.model_id), Some(FLUX));
    assert!(!q.ask_render_stop(FLUX), "nothing is left to ask");
    assert!(!q.render_stop_asked(FLUX));
}

/// A render still waiting for its turn when its model is stopped leaves the
/// line with its lease, released on the slot it was counted in, and the
/// chats it queued behind it go on.
#[tokio::test]
async fn a_stop_sends_a_waiting_render_out_of_the_line() {
    let (q, lease) = chat_and_image();
    let gate = q.generation_gate();
    let in_flight = q.lease(PRIMARY_SLOT).expect("qwen is resident");
    let mut render = pin!(gate.render_turn(lease, None));
    assert!(poll(render.as_mut()).is_pending());
    assert_eq!(inflight(&q, 1), 1);

    // A chat that arrived after the render waits behind it.
    let (behind, decision) = chat(&q);
    assert_eq!(decision, AdmissionDecision::Wait);

    assert!(q.ask_render_stop(FLUX));
    assert!(!q.render_stop_asked(FLUX), "it never drew");

    match poll(render.as_mut()) {
        Poll::Ready(Err(GateError::Unavailable(why))) => assert_eq!(why, RENDER_STOPPED),
        other => panic!("expected the stop, got {other:?}"),
    }
    assert_eq!(q.slot(1).map(|r| r.model_id), Some(FLUX));
    assert_eq!(inflight(&q, 1), 0, "its lease went back to its own slot");
    assert!(!q.ask_render_stop(FLUX), "no render holds the model now");

    drop(in_flight);
    assert_eq!(
        q.poll(&behind, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT },
        "the chat behind it goes on"
    );
    drop(q.claim(PRIMARY_SLOT));
}
