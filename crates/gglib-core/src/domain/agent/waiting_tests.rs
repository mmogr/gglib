use super::*;

/// A wait as a frame logged before `said` was added: its fields alone.
fn unsaid(
    reason: WaitingFor,
    position: u32,
    model: Option<&str>,
    elapsed_ms: Option<u64>,
) -> AgentEvent {
    AgentEvent::Waiting {
        reason,
        step: 3,
        total: 20,
        position,
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
    let untimed = unsaid(WaitingFor::ModelLoad, 0, Some("Qwen3-4B"), None);
    assert_eq!(
        untimed.waiting_sentence().as_deref(),
        Some("Loading Qwen3-4B…")
    );
}

#[test]
fn a_load_with_no_model_named_says_the_model() {
    let load = unsaid(WaitingFor::ModelLoad, 0, None, Some(3_000));
    assert_eq!(
        load.waiting_sentence().as_deref(),
        Some("Loading the model… 3 s")
    );
}

#[test]
fn a_render_says_its_step_once_it_has_one_and_its_place_when_not_next() {
    let render = AgentEvent::image_render_wait(3, 20, 1);
    assert_eq!(
        said_of(&render),
        Some("Waiting for an image render, step 3 of 20")
    );
    assert_eq!(
        said_of(&AgentEvent::image_render_wait(0, 0, 1)),
        Some("Waiting for an image render")
    );
    assert_eq!(
        said_of(&AgentEvent::image_render_wait(3, 20, 2)),
        Some("Waiting for an image render, step 3 of 20, 2 in line")
    );
    assert_eq!(
        said_of(&AgentEvent::image_render_wait(0, 0, 3)),
        Some("Waiting for an image render, 3 in line")
    );
}

/// The frame's own words win over its fields: a reader shows what the
/// daemon said, as it said it.
#[test]
fn a_frames_said_is_its_sentence() {
    let mut wait = unsaid(WaitingFor::ModelLoad, 0, Some("Qwen3-4B"), Some(3_000));
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
