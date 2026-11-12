//! What the dashboard's projection of the queue says about a resident's
//! launch and about the room in the second slot.
//!
//! Its own module, beside `state_tests.rs`, because these are about one
//! projection rather than the scheduling rules `queue_tests.rs` covers.

use std::sync::Arc;

use gglib_core::domain::{
    CacheRamHealth, LaunchDecision, LaunchNarration, ResidencyState, SecondarySlotDecision,
    SecondarySlotStatus, secondary_headroom,
};
use tokio::time::Instant;

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

const NEVER_FITS: SecondarySlotDecision = SecondarySlotDecision::RefuseTooLarge {
    footprint_bytes: 9 * GIB,
    ceiling_bytes: 2 * GIB,
};

fn resident(model_id: u32, name: &str, narration: Option<LaunchNarration>) -> Resident {
    Resident {
        model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
        model_id,
        model_name: name.to_string(),
        context_size: 32_768,
        port: 8000 + u16::try_from(model_id).unwrap_or(0),
        projector: None,
        runtime: gglib_core::domain::RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 17 * GIB,
    }
}

fn narrated() -> LaunchNarration {
    let mut n = LaunchNarration::new("qwen-coder", Some("Q4_K_M".to_owned()), 17 * GIB);
    n.push(LaunchDecision::new(
        "kv",
        "k=q8_0 v=q8_0 -> 1.5 GiB",
        "default",
    ));
    n
}

/// A resident's row carries how it was launched: the state word, the
/// context, the weights, the quant from its narration, and the one flag line
/// every surface prints. A resident whose launch recorded nothing still has
/// facts, with the segments it can say.
#[tokio::test]
async fn a_residents_row_carries_its_launch_facts() {
    let q = Arc::new(AdmissionQueue::new());
    // An install hands back a lease that holds the slot in flight; dropping
    // it leaves the resident loaded and idle, as a finished request does.
    drop(q.install(PRIMARY_SLOT, resident(1, "qwen-coder", Some(narrated()))));
    drop(q.install(PRIMARY_SLOT + 1, resident(2, "restored", None)));

    let snapshot = q.lock().snapshot(Instant::now(), None);
    let rows = &snapshot.slots;
    assert_eq!(rows.len(), 2);

    let launch = rows[0].launch.as_ref().expect("the primary's launch");
    assert_eq!(launch.state, ResidencyState::Loaded);
    assert_eq!(launch.context_size, 32_768);
    assert_eq!(launch.weights_bytes, 17 * GIB);
    assert_eq!(launch.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(
        launch.flag_line,
        "Q4_K_M \u{b7} 32K \u{b7} k=q8_0 v=q8_0 KV \u{b7} 17.0 GiB"
    );
    assert!(launch.narration.is_some());

    let restored = rows[1]
        .launch
        .as_ref()
        .expect("a restored resident's launch");
    assert_eq!(restored.quantization, None);
    assert_eq!(restored.narration, None);
    assert_eq!(restored.flag_line, "32K \u{b7} 17.0 GiB");

    // The same facts the server list carries.
    assert_eq!(q.launch_facts(1).as_ref(), Some(launch));
    assert_eq!(q.launch_facts(9), None);
}

/// A resident's state word follows its in-flight count, as the row's does:
/// working while a lease holds the slot, loaded once the last one drops.
#[tokio::test]
async fn the_launch_facts_say_working_while_a_request_holds_the_slot() {
    let q = Arc::new(AdmissionQueue::new());
    let lease = q.install(PRIMARY_SLOT, resident(1, "qwen-coder", None));
    assert_eq!(
        q.launch_facts(1).expect("the resident").state,
        ResidencyState::Working
    );
    assert_eq!(
        q.lock().snapshot(Instant::now(), None).slots[0]
            .launch
            .as_ref()
            .unwrap()
            .state,
        ResidencyState::Working
    );

    drop(lease);
    assert_eq!(
        q.launch_facts(1).expect("the resident").state,
        ResidencyState::Loaded
    );
}

/// An empty second slot on a machine whose free VRAM is known says how much
/// a second model may take, the headroom the grant would allow; with no
/// reading it says what it always said; a refusal the last launch met, or
/// a model holding the slot, is said instead.
#[tokio::test]
async fn the_second_slot_says_the_room_for_one_more_when_the_free_vram_is_known() {
    let q = Arc::new(AdmissionQueue::new());
    let _primary = q.install(PRIMARY_SLOT, resident(1, "qwen-coder", None));

    let unknown = q.lock().snapshot(Instant::now(), None);
    assert_eq!(unknown.secondary_slot, SecondarySlotStatus::default());

    let known = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(known.secondary_slot.state, "available");
    assert_eq!(
        known.secondary_slot.headroom_bytes,
        secondary_headroom(Some(8 * GIB))
    );
    assert_eq!(
        known.secondary_slot.detail,
        "Room for one more, about 2.0 GiB."
    );

    // A refusal recorded by a launch stands, whatever the free VRAM says now.
    let ticket = q.enqueue(2, "big");
    let _ = q.poll(&ticket, NEVER_FITS);
    let refused = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(refused.secondary_slot.state, "too_large");
    assert_eq!(refused.secondary_slot.headroom_bytes, None);
    q.lock().forget(&ticket);

    // A model in the slot is what the slot says.
    let _second = q.install(PRIMARY_SLOT + 1, resident(3, "nomic", None));
    let held = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(held.secondary_slot.state, "resident");
    assert_eq!(held.secondary_slot.headroom_bytes, None);
}

/// The room line shows after the first model loads: a verdict taken for a
/// model going into an empty daemon is not a refusal of the second slot, so
/// nothing is recorded, and a refusal recorded against one primary is dropped
/// when another lands.
#[tokio::test]
async fn the_room_line_shows_once_the_first_model_has_loaded() {
    let q = Arc::new(AdmissionQueue::new());
    let ticket = q.enqueue(1, "qwen-coder");
    let decision = q.poll(&ticket, NEVER_FITS);
    assert!(
        matches!(
            decision,
            AdmissionDecision::Launch {
                slot: PRIMARY_SLOT,
                ..
            }
        ),
        "{decision:?}"
    );
    let _primary = q.install(PRIMARY_SLOT, resident(1, "qwen-coder", None));
    q.lock().forget(&ticket);

    let loaded = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(
        loaded.secondary_slot.detail, "Room for one more, about 2.0 GiB.",
        "{loaded:?}"
    );
    assert!(q.lock().second_slot_room_wanted());

    // A second model refused beside it: the refusal shows, and the room
    // reading is not wanted.
    let second = q.enqueue(2, "big");
    let _ = q.poll(&second, NEVER_FITS);
    assert_eq!(
        q.lock()
            .snapshot(Instant::now(), Some(8 * GIB))
            .secondary_slot
            .state,
        "too_large"
    );
    assert!(!q.lock().second_slot_room_wanted());
    q.lock().forget(&second);

    // A new primary: the old pairing's refusal goes with it.
    let _next = q.install(PRIMARY_SLOT, resident(3, "gemma", None));
    assert_eq!(
        q.lock()
            .snapshot(Instant::now(), Some(8 * GIB))
            .secondary_slot
            .detail,
        "Room for one more, about 2.0 GiB."
    );
}

/// The room line needs a first model: with nothing loaded there is no "one
/// more", and while the first model is still loading its verdict is not a
/// refusal of the second slot, so nothing is recorded. With both slots full
/// the slot says its resident, and no reading is wanted.
#[tokio::test]
async fn the_room_line_needs_a_loaded_primary_and_an_empty_second_slot() {
    let q = Arc::new(AdmissionQueue::new());
    assert!(!q.lock().second_slot_room_wanted());
    let empty = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(
        empty.secondary_slot,
        SecondarySlotStatus::default(),
        "{empty:?}"
    );

    // The first model, refused the second slot by the arithmetic while the
    // primary is empty: it takes the primary, and the refusal is not the
    // second slot's to show.
    let ticket = q.enqueue(1, "qwen-coder");
    let decision = q.poll(&ticket, NEVER_FITS);
    assert!(matches!(
        decision,
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            ..
        }
    ));
    let loading = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(
        loading.secondary_slot,
        SecondarySlotStatus::default(),
        "{loading:?}"
    );
    assert!(!q.lock().second_slot_room_wanted());

    let _primary = q.install(PRIMARY_SLOT, resident(1, "qwen-coder", None));
    q.lock().forget(&ticket);
    assert!(q.lock().second_slot_room_wanted());

    // Both slots full: the resident, and no reading wanted.
    let _second = q.install(PRIMARY_SLOT + 1, resident(2, "nomic", None));
    assert!(!q.lock().second_slot_room_wanted());
    let full = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(full.secondary_slot.state, "resident", "{full:?}");
}

/// A second model granted the slot and still loading: the slot is not room,
/// it says the load; once the model lands it is the resident; once it is
/// unloaded the room line is back, since a grant records nothing that
/// outlives the load.
#[tokio::test]
async fn a_second_slot_being_loaded_is_not_room() {
    let q = Arc::new(AdmissionQueue::new());
    let _primary = q.install(PRIMARY_SLOT, resident(1, "qwen-coder", None));
    let ticket = q.enqueue(2, "nomic");
    let decision = q.poll(
        &ticket,
        SecondarySlotDecision::Grant {
            footprint_bytes: GIB / 4,
            headroom_bytes: 8 * GIB,
        },
    );
    assert!(
        matches!(decision, AdmissionDecision::Launch { slot, .. } if slot == PRIMARY_SLOT + 1),
        "{decision:?}"
    );
    assert!(!q.lock().second_slot_room_wanted());
    let loading = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(
        loading.secondary_slot,
        SecondarySlotStatus::loading(),
        "{loading:?}"
    );

    let lease = q.install(PRIMARY_SLOT + 1, resident(2, "nomic", None));
    q.lock().forget(&ticket);
    assert_eq!(
        q.lock()
            .snapshot(Instant::now(), Some(8 * GIB))
            .secondary_slot
            .state,
        "resident"
    );

    drop(lease);
    assert!(q.evict(PRIMARY_SLOT + 1).is_some());
    assert!(q.lock().second_slot_room_wanted());
    assert_eq!(
        q.lock()
            .snapshot(Instant::now(), Some(8 * GIB))
            .secondary_slot
            .detail,
        "Room for one more, about 2.0 GiB."
    );
}

/// The free VRAM is read only when the room line can show: not on an empty
/// daemon, and once a first model is loaded.
#[tokio::test]
async fn the_free_vram_is_read_only_when_the_room_line_can_show() {
    let q = Arc::new(AdmissionQueue::new());
    let read = std::cell::Cell::new(0);
    let snapshot = q.snapshot_reading(|| {
        read.set(read.get() + 1);
        Some(8 * GIB)
    });
    assert_eq!(read.get(), 0, "{snapshot:?}");
    assert_eq!(snapshot.secondary_slot, SecondarySlotStatus::default());

    let _primary = q.install(PRIMARY_SLOT, resident(1, "qwen-coder", None));
    let snapshot = q.snapshot_reading(|| {
        read.set(read.get() + 1);
        Some(8 * GIB)
    });
    assert_eq!(read.get(), 1);
    assert_eq!(
        snapshot.secondary_slot.detail,
        "Room for one more, about 2.0 GiB."
    );
}

/// An image model's first load on an empty daemon goes into the second slot
/// (never an empty primary); that is no second model, so the slot says
/// nothing of a load, and no room.
#[tokio::test]
async fn a_first_load_into_the_second_slot_is_no_second_model() {
    let q = Arc::new(AdmissionQueue::new());
    let ticket = q.enqueue(5, "flux");
    let decision = q.poll(
        &ticket,
        Candidate::image(SecondarySlotDecision::RefuseUnknownBudget),
    );
    assert!(
        matches!(decision, AdmissionDecision::Launch { slot, .. } if slot == PRIMARY_SLOT + 1),
        "{decision:?}"
    );
    let loading = q.lock().snapshot(Instant::now(), Some(8 * GIB));
    assert_eq!(
        loading.secondary_slot,
        SecondarySlotStatus::default(),
        "{loading:?}"
    );
    assert!(!q.lock().second_slot_room_wanted());
}
