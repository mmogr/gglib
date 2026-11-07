use super::*;

fn wait(reason: WaitingFor, model: Option<&str>, elapsed_ms: Option<u64>) -> AgentEvent {
    AgentEvent::Waiting {
        reason,
        step: 3,
        total: 20,
        position: 0,
        model: model.map(str::to_owned),
        elapsed_ms,
    }
}

#[test]
fn a_load_names_its_model_and_whole_seconds() {
    let load = wait(WaitingFor::ModelLoad, Some("Qwen3-4B"), Some(12_900));
    assert_eq!(
        load.waiting_sentence().as_deref(),
        Some("Loading Qwen3-4B… 12 s")
    );
}

#[test]
fn a_load_under_a_second_or_untimed_says_no_seconds() {
    for elapsed in [Some(999), None] {
        let load = wait(WaitingFor::ModelLoad, Some("Qwen3-4B"), elapsed);
        assert_eq!(
            load.waiting_sentence().as_deref(),
            Some("Loading Qwen3-4B…"),
            "{elapsed:?}"
        );
    }
}

#[test]
fn a_load_with_no_model_named_says_the_model() {
    let load = wait(WaitingFor::ModelLoad, None, Some(3_000));
    assert_eq!(
        load.waiting_sentence().as_deref(),
        Some("Loading the model… 3 s")
    );
}

#[test]
fn a_render_says_its_step_once_it_has_one() {
    let render = wait(WaitingFor::ImageRender, None, None);
    assert_eq!(
        render.waiting_sentence().as_deref(),
        Some("waiting for an image render, step 3 of 20")
    );
    let unstepped = AgentEvent::Waiting {
        reason: WaitingFor::ImageRender,
        step: 0,
        total: 0,
        position: 1,
        model: None,
        elapsed_ms: None,
    };
    assert_eq!(
        unstepped.waiting_sentence().as_deref(),
        Some("waiting for an image render")
    );
}

#[test]
fn every_other_event_has_no_sentence() {
    let answer = AgentEvent::FinalAnswer {
        content: "x".to_owned(),
    };
    assert_eq!(answer.waiting_sentence(), None);
}

/// A logged `waiting` frame, as `contracts/runs/recorded.json` pins it,
/// reads back as the event it was written from.
#[test]
fn a_logged_wait_reads_back() {
    let frame = r#"{"elapsed_ms":1000,"model":"Qwen3-4B","position":0,"reason":"model_load","step":0,"total":0,"type":"waiting"}"#;
    let event: AgentEvent = serde_json::from_str(frame).expect("a waiting frame");
    assert_eq!(
        event.waiting_sentence().as_deref(),
        Some("Loading Qwen3-4B… 1 s")
    );
}
