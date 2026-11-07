//! A turn through the daemon's door, against a stand-in daemon: what is
//! put, how the run's frames are read back, what a refusal says, and that
//! Ctrl+C cancels the run.

use gglib_core::domain::attachment::AttachmentId;
use gglib_core::domain::runs::RunError;
use serde_json::{Value, json};

use super::turn_stand_in::{Answer, handle, run, run_id, stand_in};
use super::*;
use crate::daemon_client::STAND_IN_PORT;

/// What the daemon logs of an agent run, as `contracts/runs/recorded.json`
/// pins it for every client of the door.
const RECORDED: &str = include_str!("../../../../../contracts/runs/recorded.json");
/// A drawing turn's progress, as `contracts/runs/tool_progress.json` pins it.
const TOOL_PROGRESS: &str = include_str!("../../../../../contracts/runs/tool_progress.json");
/// A turn's usage between its words, as `contracts/runs/turn_made.json` pins it.
const TURN_MADE: &str = include_str!("../../../../../contracts/runs/turn_made.json");

/// The recorded frames of a turn that drew a picture and answered.
pub(crate) fn recorded_reply() -> Vec<String> {
    let recorded: Value = serde_json::from_str(RECORDED).expect("the contract reads");
    let frames = recorded["tool_reply"].as_array().expect("a reply's frames");
    frames.iter().map(Value::to_string).collect()
}

/// A turn on chat 7 that says `content` and nothing more.
pub(crate) fn plain(content: &str) -> LocalTurn {
    LocalTurn {
        conversation_id: 7,
        content: content.to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
        image_model: None,
        folder: None,
    }
}

const QUIET: Shown = Shown {
    verbose: false,
    quiet: true,
};

/// Every frame the daemon logs of a turn, as the contracts pin them, and
/// one of each kind they carry none of, reads back as the event it was
/// written from, and writes out as the same frame: what the session renders
/// is what the daemon ran. A frame that did not read would be skipped, and
/// the session would show nothing of it.
#[test]
fn every_logged_frame_reads_back_as_the_event_it_was_logged_from() {
    let recorded: Value = serde_json::from_str(RECORDED).unwrap();
    let progress: Value = serde_json::from_str(TOOL_PROGRESS).unwrap();
    let made: Value = serde_json::from_str(TURN_MADE).unwrap();
    let mut frames = recorded["tool_reply"].as_array().unwrap().clone();
    frames.push(recorded["model_load"].clone());
    frames.extend(progress.as_array().unwrap().iter().cloned());
    frames.extend(made["frames"].as_array().unwrap().iter().cloned());
    frames.extend([
        json!({"type": "reasoning_delta", "content": "Let me think."}),
        json!({"type": "tool_call_writing", "tool_name": "builtin:generate_image",
               "display_name": "Generate Image", "tokens": 12}),
        json!({"type": "prompt_progress", "processed": 2048, "total": 8192, "cached": 1024,
               "time_ms": 900}),
        json!({"type": "system_warning", "message": "Too many tool calls at once.",
               "suggested_action": "gglib config settings set --max-parallel 8"}),
        json!({"type": "error", "message": "The model stopped answering."}),
    ]);
    let kinds: std::collections::BTreeSet<&str> =
        frames.iter().filter_map(|f| f["type"].as_str()).collect();
    assert_eq!(kinds.len(), 13, "every kind a run logs: {kinds:?}");

    for frame in frames {
        let event: AgentEvent = serde_json::from_value(frame.clone())
            .unwrap_or_else(|e| panic!("{frame} does not read: {e}"));
        assert_eq!(serde_json::to_value(&event).unwrap(), frame);
    }
}

/// A turn is put to `/api/runs/{id}?kind=agent` with exactly the chat
/// page's body, and its recorded reply, shown as it arrives, gives its
/// answer.
#[tokio::test]
async fn a_turn_puts_the_pages_body_and_its_recorded_reply_answers() {
    let (port, taken) = stand_in(Answer::Runs {
        frames: recorded_reply(),
        status: RunStatus::Completed,
        error: None,
    });
    let image = AttachmentId::of(b"a picture");
    let turn = LocalTurn {
        images: vec![image.clone()],
        thinking: Some(gglib_core::domain::Thinking::Off),
        draw: true,
        image_model: Some(3),
        folder: Some("/work/app".to_owned()),
        ..plain("draw a red dot")
    };

    let answered = STAND_IN_PORT
        .scope(port, send(&handle(), &turn, QUIET))
        .await;

    assert!(answered.expect("the turn ran"), "the reply gave its answer");
    let taken = taken.lock().unwrap().clone();
    let (line, body) = &taken[0];
    let id = run_id(line);
    assert!(id.starts_with("run-"), "{line}");
    assert_eq!(line, &format!("PUT /api/runs/{id}?kind=agent HTTP/1.1"));
    let body: Value = serde_json::from_str(body).expect("a JSON body");
    assert_eq!(
        body,
        json!({
            "conversation_id": 7,
            "content": "draw a red dot",
            "images": [image.as_str()],
            "thinking": "off",
            "draw": true,
            "image_model": 3,
            "folder": "/work/app",
        })
    );
    assert_eq!(
        taken[1].0,
        format!("GET /api/runs/{id}/events?after=0 HTTP/1.1")
    );
}

/// A turn that says nothing beyond its words sends nothing else: the keys
/// it leaves out are the daemon's to decide.
#[tokio::test]
async fn a_plain_turn_sends_its_words_and_its_chat_alone() {
    let (port, taken) = stand_in(Answer::Runs {
        frames: vec![json!({"type": "final_answer", "content": "hi"}).to_string()],
        status: RunStatus::Completed,
        error: None,
    });

    let answered = STAND_IN_PORT
        .scope(port, send(&handle(), &plain("hello"), QUIET))
        .await;

    assert!(answered.unwrap());
    let body = taken.lock().unwrap()[0].1.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"conversation_id": 7, "content": "hello"})
    );
}

/// A turn the daemon refuses before it runs is an error in the daemon's own
/// words, and no run's events are read.
#[tokio::test]
async fn a_refused_turn_is_the_daemons_sentence() {
    let said = "Model 'qwen' cannot read images. Choose a model that can, or send the message \
                without its images.";
    let (port, taken) = stand_in(Answer::Refuses(
        422,
        json!({"error": said, "code": "model_cannot_read_images"}),
    ));
    let turn = LocalTurn {
        images: vec![AttachmentId::of(b"a picture")],
        ..plain("what is this?")
    };

    let refused = STAND_IN_PORT
        .scope(port, send(&handle(), &turn, QUIET))
        .await;

    assert_eq!(refused.expect_err("refused").to_string(), said);
    assert_eq!(taken.lock().unwrap().len(), 1, "nothing after the refusal");
}

/// A run that failed is no answer.
#[tokio::test]
async fn a_failed_run_is_no_answer() {
    let (port, _) = stand_in(Answer::Runs {
        frames: vec![json!({"type": "text_delta", "content": "Half"}).to_string()],
        status: RunStatus::Failed,
        error: Some(RunError {
            code: "model_unavailable".to_owned(),
            message: "The model could not be loaded.".to_owned(),
        }),
    });

    let answered = STAND_IN_PORT
        .scope(port, send(&handle(), &plain("hello"), QUIET))
        .await;

    assert!(!answered.expect("the turn was taken"));
}

/// Why a failed run failed is the daemon's sentence; a run that did not
/// fail has no such sentence.
#[test]
fn a_failed_run_says_why_in_the_daemons_words() {
    let error = RunError {
        code: "model_unavailable".to_owned(),
        message: "The model could not be loaded.".to_owned(),
    };
    let failed = run("r1", RunStatus::Failed, Some(error));
    let unsaid = run("r2", RunStatus::Failed, None);

    assert_eq!(
        failure(&failed).as_deref(),
        Some("The model could not be loaded.")
    );
    assert_eq!(
        failure(&unsaid).as_deref(),
        Some("the reply failed, and the daemon gave no reason")
    );
    assert_eq!(failure(&run("r3", RunStatus::Completed, None)), None);
    assert_eq!(failure(&run("r4", RunStatus::Cancelled, None)), None);
}

/// Ctrl+C while a reply is being shown cancels its run on the daemon, and
/// is no answer.
#[tokio::test]
async fn ctrl_c_cancels_the_run_on_the_daemon() {
    let (port, taken) = stand_in(Answer::Runs {
        frames: Vec::new(),
        status: RunStatus::Completed,
        error: None,
    });

    let stopped = STAND_IN_PORT
        .scope(
            port,
            follow(&handle(), "run-0123abcd", QUIET, async { Ok(()) }),
        )
        .await;

    assert!(!stopped);
    let lines: Vec<String> = taken.lock().unwrap().iter().map(|t| t.0.clone()).collect();
    assert!(
        lines.contains(&"POST /api/runs/run-0123abcd/cancel HTTP/1.1".to_owned()),
        "{lines:?}"
    );
}
