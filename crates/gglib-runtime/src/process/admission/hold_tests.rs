//! A held resident is neither swapped out nor recycled, yet still serves.
//!
//! Helpers are duplicated from `queue_tests.rs`, which the complexity ratchet
//! holds at its size.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::domain::{CacheRamHealth, SecondarySlotDecision};
use gglib_core::ports::ModelRuntimeError;
use tokio::time::Instant;

use crate::process::admission::{AdmissionDecision, AdmissionQueue, PRIMARY_SLOT, Resident};

const NEVER_FITS: SecondarySlotDecision = SecondarySlotDecision::RefuseTooLarge {
    footprint_bytes: 9 * 1024 * 1024 * 1024,
    ceiling_bytes: 2 * 1024 * 1024 * 1024,
};

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

/// A queue whose primary holds model 1, `qwen-coder`, at port 8001, idle.
fn with_resident() -> Arc<AdmissionQueue> {
    let q = Arc::new(AdmissionQueue::new());
    drop(q.install(PRIMARY_SLOT, resident(1, "qwen-coder")));
    q
}

#[tokio::test]
async fn a_held_resident_is_not_swapped_out_until_the_hold_drops() {
    let q = with_resident();
    let hold = q.hold(8001, 1).expect("a resident listens there");

    let rival = q.enqueue("nomic-embed");
    assert_eq!(q.poll(&rival, NEVER_FITS), AdmissionDecision::Wait);

    drop(hold);
    assert_eq!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: Some(1)
        }
    );
}

#[tokio::test]
async fn a_held_resident_still_serves_its_own_requests() {
    let q = with_resident();
    let _hold = q.hold(8001, 1).unwrap();
    // A rival waiting would make an idle, unheld slot stand aside.
    let _rival = q.enqueue("nomic-embed");

    let ticket = q.enqueue("qwen-coder");
    assert_eq!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT }
    );
}

#[tokio::test]
async fn a_held_resident_is_not_recycled_until_every_hold_drops() {
    let q = with_resident();
    let first = q.hold(8001, 1).unwrap();
    let second = q.hold(8001, 1).unwrap();

    let refused = q.evict_unheld(PRIMARY_SLOT).unwrap_err();
    assert!(matches!(refused, ModelRuntimeError::AdmissionTimeout(_)));
    assert!(refused.is_retryable());
    assert!(
        refused.to_string().contains("'qwen-coder' is held"),
        "{refused}"
    );
    drop(first);
    assert!(q.evict_unheld(PRIMARY_SLOT).is_err(), "one hold is left");
    assert!(q.slot(PRIMARY_SLOT).is_some());

    drop(second);
    let evicted = q.evict_unheld(PRIMARY_SLOT).unwrap();
    assert_eq!(evicted.map(|r| r.model_id), Some(1));
}

#[tokio::test]
async fn nothing_is_held_on_a_port_no_resident_listens_on() {
    let q = with_resident();
    assert!(q.hold(8002, 1).is_none());
    assert!(q.evict_unheld(PRIMARY_SLOT).unwrap().is_some());
}

/// A run holds the model it resolved, not whatever now listens on its port.
#[tokio::test]
async fn a_hold_names_its_model_as_well_as_its_port() {
    let q = with_resident();
    assert!(q.hold(8001, 2).is_none(), "model 2 is not on 8001");
    assert!(q.evict_unheld(PRIMARY_SLOT).unwrap().is_some());

    drop(q.install(PRIMARY_SLOT, resident(1, "qwen-coder")));
    assert!(q.hold(8001, 1).is_some());
}

/// Ports are reused: a model stopped while held, then another loaded on
/// the same port, leaves the newcomer unheld.
#[tokio::test]
async fn a_hold_on_a_stopped_model_does_not_hold_the_next_on_its_port() {
    let q = with_resident();
    let stale = q.hold(8001, 1).unwrap();
    assert!(q.evict(PRIMARY_SLOT).is_some(), "an explicit stop");

    let next = || Resident {
        port: 8001,
        ..resident(2, "nomic-embed")
    };
    drop(q.install(PRIMARY_SLOT, next()));
    let rival = q.enqueue("llama");
    assert_eq!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: Some(2)
        },
        "the newcomer is swapped out as any idle model is"
    );
    drop(rival);

    drop(q.install(PRIMARY_SLOT, next()));
    assert!(q.evict_unheld(PRIMARY_SLOT).is_ok(), "and recycled");
    drop(stale);
}
