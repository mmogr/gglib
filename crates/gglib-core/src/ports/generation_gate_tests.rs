//! A turn and the lease it owns: what each call reaches, and in what order.

use std::sync::Mutex;

use super::*;
use crate::ports::AdmissionRelease;

/// Records every call made on the gate and the queue, in order.
#[derive(Debug, Default)]
struct Log(Mutex<Vec<String>>);

impl Log {
    fn push(&self, event: String) {
        self.0.lock().unwrap().push(event);
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

impl AdmissionRelease for Log {
    fn release(&self, slot: usize) {
        self.push(format!("release {slot}"));
    }

    fn progress(&self, slot: usize) {
        self.push(format!("lease progress {slot}"));
    }
}

impl GateRelease for Log {
    fn progress(&self, id: u64, step: u32, total: u32) {
        self.push(format!("turn {id} step {step}/{total}"));
    }

    fn end(&self, id: u64) {
        self.push(format!("end {id}"));
    }
}

fn render(log: &Arc<Log>) -> GenerationTurn {
    let lease = AdmissionLease::new(Arc::clone(log) as Arc<dyn AdmissionRelease>, 1);
    GenerationTurn::new(
        Arc::clone(log) as Arc<dyn GateRelease>,
        7,
        TurnKind::Render,
        Some(lease),
    )
}

/// A step is kept for waiters first, then counted as queue progress through
/// the lease, on the slot the lease holds.
#[test]
fn a_step_reaches_the_gate_and_then_the_lease() {
    let log = Arc::new(Log::default());
    let turn = render(&log);
    turn.progress(3, 20);
    assert_eq!(log.take(), ["turn 7 step 3/20", "lease progress 1"]);
    drop(turn);
}

/// Dropping a render turn ends it before its lease is released, the reverse
/// of the order they were taken in; each happens once.
#[test]
fn a_dropped_turn_ends_then_releases_its_lease() {
    let log = Arc::new(Log::default());
    let turn = render(&log);
    assert_eq!(turn.id(), 7);
    assert_eq!(turn.kind(), TurnKind::Render);
    assert_eq!(turn.lease().map(AdmissionLease::slot), Some(1));
    drop(turn);
    assert_eq!(log.take(), ["end 7", "release 1"]);
}

/// A teardown that takes the lease and disarms it still ends the turn, and
/// releases nothing.
#[test]
fn a_disarmed_lease_releases_nothing() {
    let log = Arc::new(Log::default());
    let mut turn = render(&log);
    let lease = turn.take_lease().expect("a render turn owns its lease");
    assert!(turn.lease().is_none());
    lease.disarm();
    drop(turn);
    assert_eq!(log.take(), ["end 7"]);
}

/// An LLM turn has no lease: a step reaches only the gate.
#[test]
fn an_llm_turn_has_no_lease_to_report_to() {
    let log = Arc::new(Log::default());
    let turn = GenerationTurn::new(
        Arc::clone(&log) as Arc<dyn GateRelease>,
        2,
        TurnKind::Llm,
        None,
    );
    turn.progress(1, 1);
    drop(turn);
    assert_eq!(log.take(), ["turn 2 step 1/1", "end 2"]);
}

/// A lease's progress goes to its owner with its slot; a detached lease has
/// no owner to tell.
#[test]
fn a_lease_reports_progress_on_its_slot() {
    let log = Arc::new(Log::default());
    let lease = AdmissionLease::new(Arc::clone(&log) as Arc<dyn AdmissionRelease>, 1);
    lease.progress();
    drop(lease);
    AdmissionLease::detached().progress();
    assert_eq!(log.take(), ["lease progress 1", "release 1"]);
}
