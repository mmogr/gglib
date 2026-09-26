//! Tests for what [`super::run`] records into the loop guard's log: one scan
//! for every request it scanned, and one decision for every request it acted
//! on, both stamped with the same moment.

use std::sync::Mutex;

use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::domain::loop_guard_log::LoopGuardTripEvent;
use gglib_core::ports::LoopGuardTripSink;
use gglib_core::{LoopGuardMode, Settings};
use serde_json::json;

use super::fixtures::{MODEL, body, in_mode, looping, stagnating, store};
use super::{GuardObservers, GuardStep, run};
use crate::loop_guard::{LoopGuardConfig, LoopGuardVerdict, scan_history};

/// What the guard's own scan concludes about `request` — the facts a logged
/// decision must carry, read here rather than restated as literals.
fn verdict_of(settings: &Settings, request: &bytes::Bytes) -> LoopGuardVerdict {
    let config = LoopGuardConfig::from_settings(settings).expect("the guard is on");
    scan_history(request, &config).verdict
}

/// A sink that keeps everything it is handed.
#[derive(Default)]
struct Recorder {
    trips: Mutex<Vec<LoopGuardTripEvent>>,
    scans: Mutex<Vec<(String, LoopGuardMode, u64)>>,
}

impl LoopGuardTripSink for Recorder {
    fn record_trip(&self, event: LoopGuardTripEvent) {
        self.trips.lock().unwrap().push(event);
    }

    fn record_scan(&self, model_name: &str, mode: LoopGuardMode, at_secs: u64) {
        self.scans
            .lock()
            .unwrap()
            .push((model_name.to_owned(), mode, at_secs));
    }
}

/// Run the step with `sink` behind it and a session id of `s1`.
fn step_into(sink: &Recorder, settings: &Settings, request: &bytes::Bytes) -> GuardStep {
    let (metrics, _ledger) = store();
    run(
        settings,
        request,
        MODEL,
        &GuardObservers {
            metrics: &metrics,
            trips: Some(sink),
            session_id: Some("s1"),
        },
    )
}

#[test]
fn a_guard_that_is_off_records_neither_a_scan_nor_a_trip() {
    let sink = Recorder::default();
    step_into(&sink, &in_mode(LoopGuardMode::Off), &body(looping(3)));

    assert!(sink.scans.lock().unwrap().is_empty());
    assert!(sink.trips.lock().unwrap().is_empty());
}

#[test]
#[allow(
    clippy::significant_drop_tightening,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn a_scan_with_no_trip_is_one_scan_and_no_decision() {
    let sink = Recorder::default();
    let step = step_into(
        &sink,
        &Settings::with_defaults(),
        &body(vec![json!({ "role": "assistant", "content": "done" })]),
    );

    assert!(matches!(step, GuardStep::Forward));
    let scans = sink.scans.lock().unwrap();
    assert_eq!(scans.len(), 1);
    assert_eq!(scans[0].0, "test-model");
    assert_eq!(scans[0].1, LoopGuardMode::Note);
    assert!(sink.trips.lock().unwrap().is_empty());
}

#[test]
#[allow(
    clippy::significant_drop_tightening,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn a_noted_loop_is_one_scan_and_one_noted_decision_at_the_same_moment() {
    let sink = Recorder::default();
    let request = body(looping(3));
    let step = step_into(&sink, &Settings::with_defaults(), &request);

    assert!(matches!(step, GuardStep::Note { .. }));
    let LoopGuardVerdict::LoopDetected { signature } =
        verdict_of(&Settings::with_defaults(), &request)
    else {
        panic!("three identical batches are a loop");
    };
    let scans = sink.scans.lock().unwrap();
    let trips = sink.trips.lock().unwrap();
    assert_eq!((scans.len(), trips.len()), (1, 1));
    let event = &trips[0];
    assert_eq!(event.mode(), LoopGuardMode::Note);
    assert_eq!(event.detector(), LoopGuardTrip::Loop);
    assert_eq!(event.model_name(), "test-model");
    let expected = LoopGuardTripEvent::new(0, "other", LoopGuardTrip::Loop, LoopGuardMode::Note)
        .with_signature(&signature);
    assert_eq!(
        event.signature_hash(),
        expected.signature_hash(),
        "the hash is of the batch's signature, and of nothing else"
    );
    assert_eq!(
        event.session_hash(),
        Some("e8bc163c82eee187"),
        "the session id is kept, as its hash"
    );
    assert_eq!(
        scans[0].2,
        event.recorded_at_secs(),
        "one moment per request, so the scan and the trip land on one day"
    );
}

#[test]
#[allow(
    clippy::significant_drop_tightening,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn a_refused_stagnation_is_one_refused_decision_with_its_count() {
    let sink = Recorder::default();
    let refuse = in_mode(LoopGuardMode::Refuse);
    let request = body(stagnating(6));
    let step = step_into(&sink, &refuse, &request);

    assert!(matches!(step, GuardStep::Refuse(_)));
    let LoopGuardVerdict::StagnationDetected { count, max_steps } = verdict_of(&refuse, &request)
    else {
        panic!("six identical replies are stagnation");
    };
    assert_ne!(
        count, max_steps,
        "the two must differ for the test to tell them apart"
    );
    let trips = sink.trips.lock().unwrap();
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].mode(), LoopGuardMode::Refuse);
    assert_eq!(trips[0].detector(), LoopGuardTrip::Stagnation);
    assert_eq!(trips[0].repeat_count(), Some(u32::try_from(count).unwrap()));
    assert_eq!(
        trips[0].threshold(),
        Some(u32::try_from(max_steps).unwrap())
    );
    assert_eq!(trips[0].signature_hash(), None);
    assert_eq!(sink.scans.lock().unwrap()[0].1, LoopGuardMode::Refuse);
}

#[test]
fn no_sink_records_nothing_and_decides_the_same() {
    let (metrics, _ledger) = store();
    let step = run(
        &Settings::with_defaults(),
        &body(looping(3)),
        MODEL,
        &GuardObservers {
            metrics: &metrics,
            trips: None,
            session_id: None,
        },
    );
    assert!(matches!(step, GuardStep::Note { .. }));
}
