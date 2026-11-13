use super::*;

/// A wait as a frame logged before `said` was added: its fields alone.
fn unsaid(model: Option<&str>, elapsed_ms: Option<u64>) -> AgentEvent {
    AgentEvent::Waiting {
        reason: WaitingFor::ModelLoad,
        step: 0,
        total: 0,
        position: 0,
        model: model.map(str::to_owned),
        elapsed_ms,
        said: None,
    }
}

fn said_of(event: &AgentEvent) -> Option<&str> {
    match event {
        AgentEvent::Waiting { said, .. } => said.as_deref(),
        _ => None,
    }
}

#[test]
fn a_load_names_its_model_and_whole_seconds() {
    let load = AgentEvent::model_load_wait("Qwen3-4B", 12_900);
    assert_eq!(said_of(&load), Some("Loading Qwen3-4B… 12 s"));
    assert_eq!(
        load.waiting_sentence().as_deref(),
        Some("Loading Qwen3-4B… 12 s")
    );
}

#[test]
fn a_load_under_a_second_says_no_seconds() {
    let load = AgentEvent::model_load_wait("Qwen3-4B", 999);
    assert_eq!(said_of(&load), Some("Loading Qwen3-4B…"));
    let untimed = unsaid(Some("Qwen3-4B"), None);
    assert_eq!(
        untimed.waiting_sentence().as_deref(),
        Some("Loading Qwen3-4B…")
    );
}

#[test]
fn a_load_with_no_model_named_says_the_model() {
    let load = unsaid(None, Some(3_000));
    assert_eq!(
        load.waiting_sentence().as_deref(),
        Some("Loading the model… 3 s")
    );
}

/// The frame's own words win over its fields: a reader shows what the
/// daemon said, as it said it.
#[test]
fn a_frames_said_is_its_sentence() {
    let mut wait = unsaid(Some("Qwen3-4B"), Some(3_000));
    if let AgentEvent::Waiting { said, .. } = &mut wait {
        *said = Some("Loading Qwen3-4B… 4 s".to_owned());
    }
    assert_eq!(
        wait.waiting_sentence().as_deref(),
        Some("Loading Qwen3-4B… 4 s")
    );
}

#[test]
fn every_other_event_has_no_sentence() {
    let answer = AgentEvent::FinalAnswer {
        content: "x".to_owned(),
    };
    assert_eq!(answer.waiting_sentence(), None);
}

/// A waiting frame logged before `said` was added still reads back, and is
/// worded from its fields.
#[test]
fn a_frame_without_said_reads_back() {
    let frame = r#"{"elapsed_ms":1000,"model":"Qwen3-4B","position":0,"reason":"model_load","step":0,"total":0,"type":"waiting"}"#;
    let event: AgentEvent = serde_json::from_str(frame).expect("a waiting frame");
    assert_eq!(said_of(&event), None);
    assert_eq!(
        event.waiting_sentence().as_deref(),
        Some("Loading Qwen3-4B… 1 s")
    );
}

/// A logged frame carries its words, and reads back with them.
#[test]
fn a_logged_wait_carries_its_words() {
    let frame = serde_json::to_value(AgentEvent::model_load_wait("Qwen3-4B", 1_000)).unwrap();
    assert_eq!(frame["said"], "Loading Qwen3-4B… 1 s");
    let event: AgentEvent = serde_json::from_value(frame).expect("a waiting frame");
    assert_eq!(said_of(&event), Some("Loading Qwen3-4B… 1 s"));
}

#[test]
fn a_slot_wait_says_its_place_in_line_and_whole_seconds() {
    let wait = AgentEvent::agent_slot_wait(2, 5_400);
    assert_eq!(
        said_of(&wait),
        Some("Waiting for a free agent slot, 2 in line… 5 s")
    );
    let next = AgentEvent::agent_slot_wait(1, 300);
    assert_eq!(
        next.waiting_sentence().as_deref(),
        Some("Waiting for a free agent slot, 1 in line…")
    );
}

/// A slot's wait names no model and has no steps: only its place and time.
#[test]
fn a_slot_wait_on_the_wire() {
    let frame = serde_json::to_value(AgentEvent::agent_slot_wait(1, 2_000)).unwrap();
    assert_eq!(
        frame,
        serde_json::json!({
            "type": "waiting",
            "reason": "agent_slot",
            "step": 0,
            "total": 0,
            "position": 1,
            "elapsed_ms": 2_000,
            "said": "Waiting for a free agent slot, 1 in line… 2 s",
        })
    );
}

/// A slot's wait without `said` is worded by its reason, never as a load.
#[test]
fn a_slot_frame_without_said_is_worded_as_a_slot_wait() {
    let frame = r#"{"elapsed_ms":3000,"position":2,"reason":"agent_slot","step":0,"total":0,"type":"waiting"}"#;
    let event: AgentEvent = serde_json::from_str(frame).expect("a waiting frame");
    assert_eq!(
        event.waiting_sentence().as_deref(),
        Some("Waiting for a free agent slot, 2 in line… 3 s")
    );
}
