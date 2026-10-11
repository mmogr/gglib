//! How an image model is judged for the second slot: by live free memory
//! alone, without the ceiling that keeps large chat models in the swap path.

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

/// A Flux-sized image model: 20 GiB of files and margin, no KV.
const IMAGE: SlotFootprint = SlotFootprint {
    weights_bytes: 20 * GIB,
    kv_bytes: 0,
};

/// The same 20 GiB is refused for a model that chats and granted for one
/// that draws, on the same card.
#[test]
fn an_image_model_is_not_held_to_the_chat_ceiling() {
    assert!(matches!(
        decide_secondary_slot_for(RuntimeKind::Llama, Some(IMAGE), Some(40 * GIB)),
        SecondarySlotDecision::RefuseTooLarge { .. }
    ));
    assert_eq!(
        decide_secondary_slot_for(RuntimeKind::StableDiffusion, Some(IMAGE), Some(40 * GIB)),
        SecondarySlotDecision::Grant {
            footprint_bytes: 20 * GIB,
            headroom_bytes: 20 * GIB,
        }
    );
}

/// The live budget still binds, with the same utilisation margin.
#[test]
fn an_image_model_is_refused_without_headroom() {
    // 20 GiB fits inside 22 GiB free, but not inside 0.9 x 22 = 19.8.
    assert_eq!(
        decide_secondary_slot_for(RuntimeKind::StableDiffusion, Some(IMAGE), Some(22 * GIB)),
        SecondarySlotDecision::RefuseNoHeadroom {
            footprint_bytes: 20 * GIB,
            free_bytes: 22 * GIB,
        }
    );
}

/// An unreadable budget refuses for an image model too; the queue then falls
/// back to an ordinary swap.
#[test]
fn an_image_model_with_an_unknown_budget_is_refused() {
    assert_eq!(
        decide_secondary_slot_for(RuntimeKind::StableDiffusion, Some(IMAGE), None),
        SecondarySlotDecision::RefuseUnknownBudget
    );
    assert_eq!(
        decide_secondary_slot_for(RuntimeKind::StableDiffusion, None, Some(40 * GIB)),
        SecondarySlotDecision::RefuseUnknownFootprint
    );
}

/// The chat rule is the one `decide_secondary_slot` has always applied.
#[test]
fn the_plain_decision_is_the_chat_rule() {
    for free in [None, Some(GIB), Some(40 * GIB)] {
        assert_eq!(
            decide_secondary_slot(Some(IMAGE), free),
            decide_secondary_slot_for(RuntimeKind::Llama, Some(IMAGE), free)
        );
    }
}

/// The bytes a refusal reports are read from the verdict.
#[test]
fn a_verdict_reports_the_bytes_it_was_judged_on() {
    let grant = SecondarySlotDecision::Grant {
        footprint_bytes: 3 * GIB,
        headroom_bytes: 5 * GIB,
    };
    assert_eq!(grant.footprint_bytes(), Some(3 * GIB));
    assert_eq!(grant.free_bytes(), Some(8 * GIB));

    let short = SecondarySlotDecision::RefuseNoHeadroom {
        footprint_bytes: 9 * GIB,
        free_bytes: 4 * GIB,
    };
    assert_eq!(short.footprint_bytes(), Some(9 * GIB));
    assert_eq!(short.free_bytes(), Some(4 * GIB));

    let large = SecondarySlotDecision::RefuseTooLarge {
        footprint_bytes: 9 * GIB,
        ceiling_bytes: 2 * GIB,
    };
    assert_eq!(large.footprint_bytes(), Some(9 * GIB));
    assert_eq!(large.free_bytes(), None);

    assert_eq!(
        SecondarySlotDecision::RefuseUnknownBudget.footprint_bytes(),
        None
    );
    assert_eq!(
        SecondarySlotDecision::RefuseUnknownBudget.free_bytes(),
        None
    );
    assert_eq!(
        SecondarySlotDecision::RefuseUnknownFootprint.footprint_bytes(),
        None
    );
}
