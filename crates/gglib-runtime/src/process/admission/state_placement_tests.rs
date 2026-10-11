//! Where an image model is placed, and what a held slot does to the line.
//!
//! Driven through the real [`AdmissionQueue`], as `queue_tests.rs` is; its
//! own module because that file is frozen at its size, so the helpers are
//! duplicated.

use std::sync::Arc;

use gglib_core::domain::{CacheRamHealth, RuntimeKind, SecondarySlotDecision};
use tokio::time::Instant;

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

/// A model that chats and may not co-reside: every change is a swap.
const NEVER_FITS: SecondarySlotDecision = SecondarySlotDecision::RefuseTooLarge {
    footprint_bytes: 9 * GIB,
    ceiling_bytes: 2 * GIB,
};

/// A small model that may co-reside.
const ALWAYS_FITS: SecondarySlotDecision = SecondarySlotDecision::Grant {
    footprint_bytes: GIB / 4,
    headroom_bytes: 8 * GIB,
};

/// An image model with room beside what is resident.
const IMAGE_FITS: Candidate = Candidate::image(SecondarySlotDecision::Grant {
    footprint_bytes: 20 * GIB,
    headroom_bytes: 4 * GIB,
});

/// An image model with no room beside what is resident.
const IMAGE_NO_ROOM: Candidate = Candidate::image(SecondarySlotDecision::RefuseNoHeadroom {
    footprint_bytes: 20 * GIB,
    free_bytes: 6 * GIB,
});

/// An image model on a machine whose free memory cannot be read.
const IMAGE_UNKNOWN: Candidate = Candidate::image(SecondarySlotDecision::RefuseUnknownBudget);

fn queue() -> Arc<AdmissionQueue> {
    Arc::new(AdmissionQueue::new())
}

fn resident(model_id: u32, name: &str) -> Resident {
    Resident {
        model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
        model_id,
        model_name: name.to_string(),
        context_size: 4096,
        port: 8000 + u16::try_from(model_id).unwrap_or(0),
        projector: None,
        runtime: RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 512 * 1024 * 1024,
    }
}

/// Launch `model` into the primary and leave it resident and idle.
fn chat_resident(q: &Arc<AdmissionQueue>, model: &str, model_id: u32) {
    let ticket = q.enqueue(model);
    let decision = q.poll(&ticket, NEVER_FITS);
    assert_eq!(
        decision,
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: None
        },
        "precondition: {model} launches into the primary"
    );
    drop(ticket);
    drop(q.install(PRIMARY_SLOT, resident(model_id, model)));
}

fn poll_once(q: &Arc<AdmissionQueue>, model: &str, candidate: Candidate) -> AdmissionDecision {
    let ticket = q.enqueue(model);
    q.poll(&ticket, candidate)
}

/// A cold daemon: the image model takes the second slot and the primary stays
/// empty for the chat model that comes next, whatever the budget says.
#[tokio::test]
async fn on_a_cold_daemon_an_image_model_takes_the_second_slot() {
    for candidate in [IMAGE_UNKNOWN, IMAGE_NO_ROOM, IMAGE_FITS] {
        let q = queue();
        assert_eq!(
            poll_once(&q, "flux", candidate),
            AdmissionDecision::Launch {
                slot: 1,
                evict: None
            },
            "{candidate:?}"
        );
        assert!(
            q.slot(PRIMARY_SLOT).is_none() && q.slot_of("flux") == Some(1),
            "the primary stays empty"
        );
    }
}

/// An empty primary is never taken, even when the second slot is busy: the
/// image model waits for it instead.
#[tokio::test]
async fn an_image_model_waits_rather_than_take_an_empty_primary() {
    let q = queue();
    // A model resident in the second slot with a request in flight, placed
    // there as an image model would be.
    let ticket = q.enqueue("embedder");
    assert!(matches!(
        q.poll(&ticket, IMAGE_UNKNOWN),
        AdmissionDecision::Launch { slot: 1, .. }
    ));
    let _busy = q.install(1, resident(7, "embedder"));

    assert_eq!(
        poll_once(&q, "flux", IMAGE_UNKNOWN),
        AdmissionDecision::Wait
    );
    assert!(q.slot(PRIMARY_SLOT).is_none() && !q.is_loading());
}

/// Granted beside a resident chat model: the second slot, nothing evicted.
#[tokio::test]
async fn a_granted_image_model_loads_beside_the_chat_model() {
    let q = queue();
    chat_resident(&q, "qwen", 1);

    assert_eq!(
        poll_once(&q, "flux", IMAGE_FITS),
        AdmissionDecision::Launch {
            slot: 1,
            evict: None
        }
    );
}

/// No room beside an idle, unheld chat model, or no reading at all: an
/// ordinary swap into the primary, not a squeeze into the second slot.
#[tokio::test]
async fn without_room_an_image_model_swaps_into_the_primary() {
    for candidate in [IMAGE_NO_ROOM, IMAGE_UNKNOWN] {
        let q = queue();
        chat_resident(&q, "qwen", 1);

        assert_eq!(
            poll_once(&q, "flux", candidate),
            AdmissionDecision::Launch {
                slot: PRIMARY_SLOT,
                evict: Some(1)
            },
            "{candidate:?}"
        );
    }
}

/// The swap into the primary answers to the turn rules like any other: the
/// image model waits out the chat model's quantum while its requests queue.
#[tokio::test]
async fn an_image_swap_waits_for_the_turn_rules() {
    tokio::time::pause();
    let q = queue();
    chat_resident(&q, "qwen", 1);
    // A request for the turn holder, queued and not yet polled.
    let _queued = q.enqueue("qwen");

    let ticket = q.enqueue("flux");
    assert_eq!(q.poll(&ticket, IMAGE_NO_ROOM), AdmissionDecision::Wait);
    tokio::time::advance(DRAIN_QUANTUM).await;
    assert_eq!(
        q.poll(&ticket, IMAGE_NO_ROOM),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: Some(1)
        }
    );
}

/// An empty second slot is taken at once, before the turn rules are asked: a
/// granted image model beside a chat model whose requests queue does not
/// wait out that model's quantum.
#[tokio::test]
async fn an_empty_second_slot_is_taken_before_the_turn_rules() {
    tokio::time::pause();
    let q = queue();
    chat_resident(&q, "qwen", 1);
    // A request for the turn holder, queued and not yet polled.
    let _queued = q.enqueue("qwen");

    assert_eq!(
        poll_once(&q, "flux", IMAGE_FITS),
        AdmissionDecision::Launch {
            slot: 1,
            evict: None
        }
    );
}

/// A held primary and no room beside it: refused on the first poll, naming
/// the held model and the bytes, with the ticket gone from the line.
#[tokio::test]
async fn a_held_primary_refuses_an_image_model_at_once() {
    let q = queue();
    chat_resident(&q, "qwen", 1);
    let _hold = q.hold(8001, 1).expect("qwen is resident on its port");

    let ticket = q.enqueue("flux");
    assert_eq!(
        q.poll(&ticket, IMAGE_NO_ROOM),
        AdmissionDecision::Refuse(Refusal::HeldSlot {
            held_model: "qwen".to_owned(),
            needed_bytes: Some(20 * GIB),
            free_bytes: Some(6 * GIB),
        })
    );
    assert!(
        q.snapshot().queued.is_empty(),
        "the refused ticket is forgotten"
    );

    // An unreadable budget is refused the same way, with no bytes to name.
    assert_eq!(
        poll_once(&q, "flux", IMAGE_UNKNOWN),
        AdmissionDecision::Refuse(Refusal::HeldSlot {
            held_model: "qwen".to_owned(),
            needed_bytes: None,
            free_bytes: None,
        })
    );
    let words = Refusal::HeldSlot {
        held_model: "qwen".to_owned(),
        needed_bytes: Some(20 * GIB),
        free_bytes: Some(6 * GIB),
    }
    .describe("flux");
    for part in [
        "'flux'",
        "'qwen'",
        "21474836480 bytes",
        "6442450944 bytes",
        "held",
    ] {
        assert!(words.contains(part), "{words:?} names {part}");
    }
}

/// A held primary does not stop a granted image model taking the second
/// slot.
#[tokio::test]
async fn a_held_primary_does_not_refuse_an_image_model_that_fits_beside_it() {
    let q = queue();
    chat_resident(&q, "qwen", 1);
    let _hold = q.hold(8001, 1).expect("qwen is resident on its port");

    assert_eq!(
        poll_once(&q, "flux", IMAGE_FITS),
        AdmissionDecision::Launch {
            slot: 1,
            evict: None
        }
    );
}

/// A waiter that only a held slot could take is passed over at the front of
/// the line, so a younger request for another model launches instead of
/// waiting out the deadline behind it; it goes once the hold ends.
#[tokio::test]
async fn a_waiter_for_a_held_slot_does_not_block_the_line() {
    let q = queue();
    chat_resident(&q, "qwen", 1);
    let hold = q.hold(8001, 1).expect("qwen is resident on its port");

    let older = q.enqueue("llama-70b");
    assert_eq!(q.poll(&older, NEVER_FITS), AdmissionDecision::Wait);

    let younger = q.enqueue("embedder");
    assert_eq!(
        q.poll(&younger, ALWAYS_FITS),
        AdmissionDecision::Launch {
            slot: 1,
            evict: None
        },
        "the held-only waiter must not hold the front of the line"
    );
    drop(younger);
    drop(q.install(1, resident(7, "embedder")));

    assert_eq!(q.poll(&older, NEVER_FITS), AdmissionDecision::Wait);
    drop(hold);
    assert_eq!(
        q.poll(&older, NEVER_FITS),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: Some(1)
        },
        "once the hold ends the older waiter goes"
    );
}

/// A model that chats is placed as before: a bare verdict is a llama
/// candidate, and an empty primary is still the first choice for it.
#[tokio::test]
async fn a_model_that_chats_still_takes_an_empty_primary() {
    let q = queue();
    let ticket = q.enqueue("qwen");
    assert_eq!(
        q.poll(&ticket, Candidate::llama(ALWAYS_FITS)),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: None
        }
    );
    assert_eq!(Candidate::from(NEVER_FITS), Candidate::llama(NEVER_FITS));
}
