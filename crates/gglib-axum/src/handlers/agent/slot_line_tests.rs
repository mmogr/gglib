//! The line for an agent slot: first come, first served, capped, and a
//! waiter that leaves moves those behind it up.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

use super::{Place, Slot, SlotLine};

/// A line over one slot, which the caller holds, with room for `cap`.
fn full(cap: usize) -> (Arc<SlotLine>, OwnedSemaphorePermit) {
    let slots = Arc::new(Semaphore::new(1));
    let held = Arc::clone(&slots).try_acquire_owned().unwrap();
    let line = Arc::new(SlotLine::new(slots, cap, Duration::from_mins(1)));
    (line, held)
}

fn join(line: &Arc<SlotLine>) -> Place {
    match line.take_or_join() {
        Some(Slot::Waiting(place)) => place,
        Some(Slot::Held(_)) => panic!("a slot was free"),
        None => panic!("the line was full"),
    }
}

/// Wait for `place`'s slot on its own task, then leave the line, as a run
/// does, and say `name` on `served`; the slot is handed back.
fn waiting(
    place: Place,
    name: &'static str,
    served: mpsc::UnboundedSender<&'static str>,
) -> tokio::task::JoinHandle<OwnedSemaphorePermit> {
    tokio::spawn(async move {
        let permit = place.slot().await.expect("a slot");
        drop(place);
        let _ = served.send(name);
        permit
    })
}

/// Let every spawned waiter run as far as it can.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn a_free_slot_is_taken_at_once_and_no_one_waits() {
    let line = Arc::new(SlotLine::new(
        Arc::new(Semaphore::new(1)),
        4,
        Duration::from_mins(1),
    ));
    assert!(matches!(line.take_or_join(), Some(Slot::Held(_))));
}

/// The places are given in arrival order, and one that leaves moves those
/// behind it up.
#[tokio::test]
async fn a_waiter_that_leaves_moves_those_behind_it_up() {
    let (line, _held) = full(4);
    let a = join(&line);
    let b = join(&line);
    let c = join(&line);
    assert_eq!([a.position(), b.position(), c.position()], [1, 2, 3]);

    drop(b);
    assert_eq!([a.position(), c.position()], [1, 2]);
    drop(a);
    assert_eq!(c.position(), 1);
}

/// The second waiter never gets the slot before the first, even when it
/// asks first: only the first in line asks the semaphore.
#[tokio::test]
async fn the_first_in_line_is_served_first_whoever_asks_first() {
    let (line, held) = full(4);
    let first = join(&line);
    let second = join(&line);
    let (served, mut order) = mpsc::unbounded_channel();
    let second = waiting(second, "second", served.clone());
    settle().await;
    let first = waiting(first, "first", served);
    settle().await;

    drop(held);
    let first_slot = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .expect("the first is served first")
        .unwrap();
    settle().await;
    assert_eq!(order.try_recv(), Ok("first"));
    assert!(
        order.try_recv().is_err(),
        "the second waits for the first's slot"
    );

    drop(first_slot);
    let _second_slot = tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .expect("the second is served once the first's slot frees")
        .unwrap();
    assert_eq!(order.try_recv(), Ok("second"));
}

/// No one takes a free slot past the line: while anyone waits, a free slot
/// is the line's.
#[tokio::test]
async fn no_one_takes_a_slot_past_the_line() {
    let (line, held) = full(4);
    let place = join(&line);
    drop(held);

    assert!(line.take().is_none(), "this machine's turn waits its turn");
    assert!(
        matches!(line.take_or_join(), Some(Slot::Waiting(_))),
        "a device's run joins the back of the line"
    );

    drop(place);
}

#[tokio::test]
async fn a_full_line_takes_no_one_else() {
    let (line, _held) = full(2);
    let _a = join(&line);
    let b = join(&line);
    assert!(line.take_or_join().is_none());

    drop(b);
    assert!(matches!(line.take_or_join(), Some(Slot::Waiting(_))));
}
