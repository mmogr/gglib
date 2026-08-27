//! What eviction does to a slot that is not a plain resident.
//!
//! Its own module because `queue_tests.rs` is frozen at its current size by the
//! complexity ratchet, and because these are about one method rather than about
//! the scheduling rules that file covers. Helpers are duplicated for the same
//! reason.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::domain::{CacheRamHealth, SecondarySlotDecision};
use tokio::time::Instant;

use super::*;

const NEVER_FITS: SecondarySlotDecision = SecondarySlotDecision::RefuseTooLarge {
    footprint_bytes: 9 * 1024 * 1024 * 1024,
    ceiling_bytes: 2 * 1024 * 1024 * 1024,
};

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
        model_path: PathBuf::from("/models/x.gguf"),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 512 * 1024 * 1024,
    }
}

/// Drive a cold queue to a slot that is mid-launch.
fn loading_primary(q: &Arc<AdmissionQueue>) {
    let ticket = q.enqueue("qwen-coder");
    let decision = q.poll(&ticket, NEVER_FITS);
    assert!(
        matches!(
            decision,
            AdmissionDecision::Launch {
                slot: PRIMARY_SLOT,
                evict: None
            }
        ),
        "precondition: a cold queue must launch, got {decision:?}"
    );
    drop(ticket);
    assert!(q.is_loading(), "precondition: the slot is mid-launch");
}

/// The defect. `evict` emptied the slot and returned `None`, so every caller —
/// each written as `if let Some(previous) = evict(..)` — killed nothing and
/// reported success, while the detached launch carried on into a slot the
/// scheduler now believed free.
#[tokio::test]
async fn evicting_a_loading_slot_leaves_the_launch_alone() {
    let q = queue();
    loading_primary(&q);

    assert!(
        q.evict(PRIMARY_SLOT).is_none(),
        "there is no resident to hand back"
    );
    assert!(
        q.is_loading(),
        "the slot must still be loading: emptying it disowns the child"
    );
}

/// The consequence that made it a leak rather than a lost kill. With the slot
/// wrongly `Empty`, `choose_slot` offered it to the next waiter and a *second*
/// llama-server spawned into one slot's worth of VRAM.
#[tokio::test]
async fn a_second_launch_cannot_claim_a_slot_that_is_still_loading() {
    let q = queue();
    loading_primary(&q);
    q.evict(PRIMARY_SLOT);

    let rival = q.enqueue("nomic-embed");
    assert_eq!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Wait,
        "a rival must wait out the load, not be handed the same slot"
    );
}

/// The launch still lands. Refusing the evict must not strand the slot: the
/// model that was loading installs normally and can then be stopped.
#[tokio::test]
async fn the_refused_launch_still_installs_and_is_then_evictable() {
    let q = queue();
    loading_primary(&q);
    q.evict(PRIMARY_SLOT);

    let lease = q.install(PRIMARY_SLOT, resident(1, "qwen-coder"));
    drop(lease);
    assert!(!q.is_loading(), "the load landed");

    let stopped = q.evict(PRIMARY_SLOT).expect("now there is a resident");
    assert_eq!(stopped.model_id, 1, "and it is the one that was loading");
}

/// Eviction of an ordinary resident is untouched — this is a guard on one
/// state, not a change to what a stop request does.
#[tokio::test]
async fn evicting_a_resident_still_hands_it_back_to_be_killed() {
    let q = queue();
    let lease = q.install(PRIMARY_SLOT, resident(7, "qwen-coder"));
    drop(lease);

    let previous = q.evict(PRIMARY_SLOT).expect("a resident was there");
    assert_eq!(previous.model_id, 7);
    assert!(q.evict(PRIMARY_SLOT).is_none(), "and the slot is now empty");
}
