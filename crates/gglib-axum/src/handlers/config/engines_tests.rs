//! Each engine's slot is its own: an install of one never refuses an install
//! or a removal of the other.

use super::*;

#[test]
fn each_engine_holds_a_slot_of_its_own() {
    assert!(!std::ptr::eq(
        slot(RuntimeKind::Llama),
        slot(RuntimeKind::StableDiffusion)
    ));
    assert!(std::ptr::eq(
        slot(RuntimeKind::Llama),
        &raw const LLAMA_IN_FLIGHT
    ));
    assert!(std::ptr::eq(
        slot(RuntimeKind::StableDiffusion),
        &raw const SD_IN_FLIGHT
    ));
}
