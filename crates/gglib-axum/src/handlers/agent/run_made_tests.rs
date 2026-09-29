//! How a run's turn was made: the model the run drove is stamped on the
//! turn's usage before it is logged, so the frame a page draws and the row
//! the daemon saves say the same.

use futures_util::StreamExt as _;
use gglib_core::domain::agent::{AgentEvent, MADE_KEYS, TurnUsage};
use gglib_core::domain::chat::MessageRole;
use gglib_core::ports::{RunEvent, RunsPort as _};
use serde_json::{Value, json};

use gglib_app_services::types::ServerInfo;

use super::remote_upstream::local;
use super::run_fixture::{End, LOCAL, conversation, meta, prepared, saved, settled, start, state};

fn turn() -> Vec<AgentEvent> {
    vec![
        AgentEvent::TextDelta {
            content: "Done.".to_owned(),
        },
        AgentEvent::TurnUsage(TurnUsage {
            prompt_tokens: Some(30),
            cached_tokens: Some(20),
            completion_tokens: Some(5),
            duration_ms: 900,
            writing_ms: Some(800),
            ..TurnUsage::default()
        }),
        AgentEvent::FinalAnswer {
            content: "Done.".to_owned(),
        },
    ]
}

#[tokio::test]
async fn the_logged_usage_names_the_model_and_the_saved_row_says_the_same() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    // The model as `resolve` gives it for a port serving a catalogued model.
    let mut entry = gglib_core::domain::NewModel::new(
        "catalogue-name".to_owned(),
        std::path::PathBuf::from("/models/served.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    entry.quantization = Some("Q4_K_M".to_owned());
    let model_id = state.core.models().add(entry).await.unwrap().id;
    let server = ServerInfo {
        model_id,
        model_name: "served-7b".to_owned(),
        pid: None,
        port: 9000,
        started_at: 0,
    };
    let req = serde_json::from_str(r#"{"port":9000,"messages":[]}"#).unwrap();
    let (mut p, _) = prepared(turn(), End::Finish);
    p.made_by = local(&state, &req, server).await.made_by;
    start(&state, "a1", Some(id), p).await;
    settled(&state).await;

    let mut events = state.runs.events(&LOCAL, "a1", 0).unwrap();
    let mut usage = None;
    while let Some(RunEvent::Frame { data, .. }) = events.next().await {
        let frame: Value = serde_json::from_str(&data).unwrap();
        if frame["type"] == "turn_usage" {
            usage = Some(frame);
        }
    }
    let usage = usage.expect("the turn's usage was logged");
    assert_eq!(
        (&usage["model"], &usage["quantization"]),
        (&json!("served-7b"), &json!("Q4_K_M"))
    );

    let rows = saved(&state, id).await;
    let reply = rows
        .iter()
        .find(|r| r.role == MessageRole::Assistant)
        .unwrap();
    let k = &MADE_KEYS;
    let pairs = [
        (k.model, "model"),
        (k.quantization, "quantization"),
        (k.prompt_tokens, "prompt_tokens"),
        (k.cached_tokens, "cached_tokens"),
        (k.completion_tokens, "completion_tokens"),
        (k.duration_ms, "duration_ms"),
        (k.writing_ms, "writing_ms"),
    ];
    for (key, field) in pairs {
        assert_eq!(meta(reply, key), usage[field], "{key}");
    }
}
