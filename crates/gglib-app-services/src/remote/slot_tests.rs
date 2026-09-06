//! Tests for the slot both sides of the tunnel occupy.
//!
//! The interesting cases are all races written out in order: a teardown
//! that lands between a reservation and its install, and a watcher that
//! comes back after its connection has been replaced. Neither is reachable
//! from `connect` or `enable` in a unit test — one wants an iroh dial and
//! the other a real proxy port — so the ordering is asserted here, where
//! the state machine is.

use super::*;

/// A slot nothing has touched is empty and reserves cleanly.
#[test]
fn an_untouched_slot_is_empty_and_reserves() {
    let mut slot = Slot::<&str>::Empty;

    assert!(slot.busy().is_none());
    assert!(slot.full().is_none());
    assert!(slot.reserve(1).is_ok());
    assert!(matches!(slot.busy(), Some(Busy::Filling)));
}

/// A second caller is refused while the first is still building, and told
/// which of the two states it met.
#[test]
fn a_reservation_refuses_the_next_caller_and_says_which_way_it_is_busy() {
    let mut slot = Slot::Empty;
    slot.reserve(1).expect("the slot was empty");

    assert!(matches!(slot.reserve(2), Err(Busy::Filling)));
    assert!(slot.install(1, "up"));
    assert!(matches!(slot.reserve(3), Err(Busy::Full)));
    assert!(matches!(slot.busy(), Some(Busy::Full)));
    assert_eq!(slot.full(), Some(&"up"));
}

/// A teardown between the reservation and the install wins, and the install
/// says so rather than putting the connection back.
///
/// This is what `gglib remote disconnect` during a dial comes down to: the
/// dial has nowhere to go when it finishes, and the value it built is its
/// own to shut down. Silently installing it would leave a port bound that
/// the person who typed `disconnect` had every reason to think was gone.
#[test]
fn a_teardown_during_a_reservation_wins_and_the_install_is_told() {
    let mut slot = Slot::Empty;
    let cancel = slot.reserve(1).expect("the slot was empty");

    assert!(matches!(slot.take(), Taken::Cancelled));
    assert!(cancel.is_cancelled(), "the slow work is told to stop");
    assert!(!slot.install(1, "up"), "the slot is no longer this dial's");
    assert!(slot.busy().is_none(), "and a lost install leaves it empty");
}

/// A failure gives the slot back — but only if it is still this call's, so
/// a release arriving after a teardown cannot wipe out the reservation that
/// replaced it.
#[test]
fn a_release_gives_the_slot_back_and_never_takes_a_later_ones() {
    let mut slot = Slot::<&str>::Empty;
    slot.reserve(1).expect("the slot was empty");
    slot.release(1);
    assert!(slot.busy().is_none());

    slot.reserve(2).expect("the slot is free again");
    slot.release(1);
    assert!(
        matches!(slot.busy(), Some(Busy::Filling)),
        "a late release from the previous call left the next one holding nothing"
    );
}

/// A watcher only takes down the connection it was watching.
///
/// `generation` exists for this: a watcher whose pipe closed a moment after
/// `disconnect` and a re-`connect` would otherwise clear the *new*
/// connection and announce a disconnection that never happened.
#[test]
fn a_stale_watcher_takes_down_nothing() {
    let mut slot = Slot::Empty;
    slot.reserve(2).expect("the slot was empty");
    slot.install(2, "generation two");

    assert!(slot.take_if(|live| *live == "generation one").is_none());
    assert_eq!(slot.full(), Some(&"generation two"));
    assert_eq!(
        slot.take_if(|live| *live == "generation two"),
        Some("generation two")
    );
    assert!(slot.busy().is_none());
}

/// A watcher that comes back while a *new* dial is in flight leaves that
/// dial alone. `take_if` looks only at a full slot, so a reservation is not
/// something a stale watcher can cancel.
#[test]
fn a_stale_watcher_does_not_cancel_a_dial_that_is_still_in_flight() {
    let mut slot = Slot::<&str>::Empty;
    let cancel = slot.reserve(3).expect("the slot was empty");

    assert!(slot.take_if(|_| true).is_none());
    assert!(!cancel.is_cancelled());
    assert!(matches!(slot.busy(), Some(Busy::Filling)));
}

/// Emptying nothing says so, which is what makes `disconnect` a conflict
/// rather than a silent success.
#[test]
fn emptying_an_empty_slot_says_it_was_empty() {
    let mut slot = Slot::<&str>::Empty;
    assert!(matches!(slot.take(), Taken::Empty));
}
