//! Tests for the step's ending (ADR 0021): under `note`, a run's second trip
//! is answered in the model's place, and what that records.

use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::{LoopGuardMode, Settings};
use serde_json::Value;

use super::fixtures::{
    MODEL, agentic_body, counts, in_mode, looping, observing, stagnating, store,
};
use super::{GuardStep, run};

#[test]
fn a_runs_first_trip_is_noted_and_its_second_is_ended() {
    for (first, second, detector) in [
        (looping(3), looping(4), LoopGuardTrip::Loop),
        (stagnating(6), stagnating(7), LoopGuardTrip::Stagnation),
    ] {
        let (metrics, _ledger) = store();
        let step = run(
            &Settings::with_defaults(),
            &agentic_body(first),
            MODEL,
            &observing(&metrics),
        );
        assert!(
            matches!(step, GuardStep::Note { .. }),
            "{detector:?}: a first trip is noted"
        );

        let (metrics, ledger) = store();
        let step = run(
            &Settings::with_defaults(),
            &agentic_body(second),
            MODEL,
            &observing(&metrics),
        );
        assert!(
            matches!(step, GuardStep::End),
            "{detector:?}: a second trip in the same run is ended"
        );
        // The step records the ended request itself, since nothing forwards it.
        let recent = metrics.recent(1);
        assert_eq!(recent.len(), 1);
        assert!(recent[0].loop_guard_ended, "{:?}", recent[0]);
        assert_eq!(recent[0].loop_guard_trip, Some(detector));
        assert_eq!(counts(&ledger).loop_guard_trips, 1, "counted once");
    }
}

/// `refuse` has no second trip to see: it acts on the first, every time.
#[test]
fn refuse_still_refuses_a_run_that_would_have_ended() {
    let (metrics, _ledger) = store();
    let step = run(
        &in_mode(LoopGuardMode::Refuse),
        &agentic_body(looping(4)),
        MODEL,
        &observing(&metrics),
    );
    assert!(matches!(step, GuardStep::Refuse(_)));
    assert!(!metrics.recent(1)[0].loop_guard_ended);
}

#[tokio::test]
async fn the_ended_answer_is_a_finished_message_with_no_tool_call() {
    for streaming in [false, true] {
        let resp = crate::loop_guard_end::ended_response(MODEL, streaming);
        assert_eq!(resp.status(), 200);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .expect("body");
        let text = String::from_utf8(bytes.to_vec()).expect("utf-8");
        if streaming {
            let frames: Vec<Value> = text
                .split("\n\n")
                .filter_map(|f| f.strip_prefix("data: "))
                .filter(|f| *f != "[DONE]")
                .map(|f| serde_json::from_str(f).expect("frame json"))
                .collect();
            assert_eq!(frames.len(), 2, "{text}");
            assert_eq!(
                frames[0]["choices"][0]["delta"]["content"],
                crate::loop_guard_end::ended_text()
            );
            assert_eq!(frames[1]["choices"][0]["finish_reason"], "stop");
            assert!(!text.contains("tool_calls"), "{text}");
            assert!(text.ends_with("data: [DONE]\n\n"), "{text}");
        } else {
            let body: Value = serde_json::from_str(&text).expect("json");
            let choice = &body["choices"][0];
            assert_eq!(choice["finish_reason"], "stop");
            assert_eq!(choice["message"]["role"], "assistant");
            assert_eq!(
                choice["message"]["content"],
                crate::loop_guard_end::ended_text()
            );
            assert!(choice["message"].get("tool_calls").is_none(), "{body}");
        }
    }
}
