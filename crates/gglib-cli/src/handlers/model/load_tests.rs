//! Tests for [`super`]: what a load and an unload send the daemon, in what
//! order, and that what they print is the daemon's answer.

use gglib_app_services::types::StartServerRequest;

use super::{load_on, unload_on};
use crate::daemon_client::stand_in::against;
use crate::daemon_client::{StartServerBody, paths};
use crate::handlers::inference::switch::Consent;

/// A consent that answers `answer`, with nobody to ask unless `terminal`.
fn consent(answer: bool, terminal: bool) -> Consent {
    Consent {
        yes: false,
        terminal,
        command: "gglib model load 3 --pin".to_owned(),
        ask: Box::new(move |_| Ok(answer)),
    }
}

fn body(id: i64, context: Option<u64>) -> StartServerBody {
    StartServerBody {
        id,
        config: StartServerRequest {
            context_length: context,
            ..Default::default()
        },
    }
}

/// A load is one start request, the model's id and context flat in its
/// body, and the line printed is the daemon's own sentence: one the CLI
/// could not have built from the port.
#[tokio::test]
async fn a_load_asks_the_daemon_to_start_the_model_and_prints_its_sentence() {
    let reply = serde_json::json!({ "port": 9301, "message": "loaded at 9301 (stand-in)" });
    let (lines, taken) = against(vec![(200, reply.to_string())], |handle| async move {
        load_on(
            &handle,
            &body(3, Some(4096)),
            false,
            &mut consent(false, false),
        )
        .await
    })
    .await;

    assert_eq!(lines.unwrap(), ["  loaded at 9301 (stand-in)"]);
    assert_eq!(taken.len(), 1);
    assert_eq!(
        taken[0].line,
        format!("POST {} HTTP/1.1", paths::SERVERS_START_PATH)
    );
    assert_eq!(taken[0].body["id"], 3);
    assert_eq!(taken[0].body["contextLength"], 4096);
}

/// With `--pin` the endpoint is pinned first and the model loaded second: a
/// plain start would bring up an unpinned endpoint that the pin then refuses.
/// The pinned body carries the same options the load does.
#[tokio::test]
async fn a_pinned_load_pins_the_endpoint_before_it_loads_the_model() {
    let pinned = serde_json::json!({
        "running": true,
        "state": "on",
        "port": 8080,
        "base_url": "http://127.0.0.1:8080/v1",
        "key": "none_needed",
        "pinned_model": "qwen3",
        "pinned_model_id": 3,
    });
    let started = serde_json::json!({ "port": 9301, "message": "loaded at 9301 (stand-in)" });
    let (lines, taken) = against(
        vec![(200, pinned.to_string()), (200, started.to_string())],
        |handle| async move {
            load_on(
                &handle,
                &body(3, Some(4096)),
                true,
                &mut consent(false, false),
            )
            .await
        },
    )
    .await;

    assert_eq!(
        lines.unwrap(),
        [
            "  Endpoint on port 8080, pinned to qwen3",
            "  loaded at 9301 (stand-in)",
        ]
    );
    let lines_taken: Vec<&str> = taken.iter().map(|t| t.line.as_str()).collect();
    assert_eq!(
        lines_taken,
        [
            format!("POST {} HTTP/1.1", paths::PROXY_START_PINNED_PATH),
            format!("POST {} HTTP/1.1", paths::SERVERS_START_PATH),
        ]
    );
    assert_eq!(taken[0].body["model_id"], 3);
    assert_eq!(taken[0].body["options"]["contextLength"], 4096);
    assert!(taken[0].body["proxy"].is_object(), "{}", taken[0].body);
    assert_eq!(taken[0].body["proxy"]["replace"], false);
}

/// The daemon's refusal of a pinned start with every model being served.
fn differs() -> (u16, String) {
    (
        409,
        serde_json::json!({
            "error": "The endpoint is on at port 8080, serving every model. ...",
            "type": paths::ENDPOINT_DIFFERS_TYPE,
            "metadata": {
                "now": "on at port 8080, serving every model",
                "action": "serve only qwen3",
            },
        })
        .to_string(),
    )
}

/// With the endpoint on otherwise, `--pin` asks as `serve` does: a yes
/// pins it with `replace` and then loads the model; a no loads nothing.
#[tokio::test]
async fn a_pin_over_an_endpoint_on_otherwise_asks_first() {
    let pinned = serde_json::json!({
        "running": true, "state": "pinned", "port": 8080,
        "base_url": "http://127.0.0.1:8080/v1", "key": "none_needed",
        "pinned_model": "qwen3", "pinned_to": "Pinned to qwen3", "pinned_model_id": 3,
        "outcome": "replaced",
    });
    let loaded = serde_json::json!({ "port": 9301, "message": "loaded at 9301 (stand-in)" });
    let (lines, taken) =
        against(
            vec![
                differs(),
                (200, pinned.to_string()),
                (200, loaded.to_string()),
            ],
            |handle| async move {
                load_on(&handle, &body(3, None), true, &mut consent(true, true)).await
            },
        )
        .await;
    assert_eq!(
        lines.unwrap(),
        [
            "  Endpoint on port 8080, pinned to qwen3",
            "  loaded at 9301 (stand-in)",
        ]
    );
    assert_eq!(taken.len(), 3);
    assert_eq!(taken[1].body["proxy"]["replace"], true);

    let (lines, taken) =
        against(
            vec![differs(), (200, loaded.to_string())],
            |handle| async move {
                load_on(&handle, &body(3, None), true, &mut consent(false, true)).await
            },
        )
        .await;
    assert_eq!(
        format!("{:#}", lines.expect_err("a no loads nothing, and fails")),
        "Nothing changed: the endpoint is still on at port 8080, serving every model."
    );
    assert_eq!(taken.len(), 1, "a no loads nothing");
}

/// A refusal is the daemon's sentence, and the model is not asked for after
/// the pin was refused: a second reply stands ready and is never taken.
#[tokio::test]
async fn a_refused_pin_is_the_daemons_sentence_and_loads_nothing() {
    let refusal = serde_json::json!({
        "error": "the proxy is already running unpinned \u{2014} stop it first (`gglib proxy stop`)"
    });
    let would_start = serde_json::json!({ "port": 9301, "message": "loaded at 9301 (stand-in)" });
    let (lines, taken) =
        against(
            vec![(409, refusal.to_string()), (200, would_start.to_string())],
            |handle| async move {
                load_on(&handle, &body(3, None), true, &mut consent(false, false)).await
            },
        )
        .await;

    let err = format!("{:#}", lines.expect_err("the daemon refused"));
    assert!(err.contains("already running unpinned"), "{err}");
    assert_eq!(taken.len(), 1, "nothing was asked after the refusal");
}

/// A model that did not load is said in the daemon's sentence alone: no
/// HTTP status, and no "did not load" before a sentence that says so and
/// why (#1959 C5). What `main` prints is the error's whole chain.
#[tokio::test]
async fn a_model_that_did_not_load_is_said_in_the_daemons_sentence_alone() {
    let sentence = "llama-server stopped while loading qwen: it exited with status 1. \
                    It printed: gguf_init_from_reader: invalid magic characters: 'this', \
                    expected 'GGUF'";
    let refusal =
        serde_json::json!({ "error": sentence, "status": 500, "type": "model_load_failed" });
    let (lines, _) = against(vec![(500, refusal.to_string())], |handle| async move {
        load_on(&handle, &body(3, None), false, &mut consent(false, false)).await
    })
    .await;

    let err = lines.expect_err("the model did not load");
    assert_eq!(format!("{err:#}"), sentence);
    assert_eq!(err.chain().count(), 1, "{err:?}");
}

/// An unload is one stop request naming the model, and the line printed is
/// the daemon's sentence; a model that is not loaded is the daemon's 404,
/// in its words.
#[tokio::test]
async fn an_unload_asks_the_daemon_to_stop_the_model_and_prints_its_sentence() {
    let (line, taken) = against(
        vec![(
            200,
            serde_json::json!("Server for model 3 stopped").to_string(),
        )],
        |handle| async move { unload_on(&handle, 3).await },
    )
    .await;
    assert_eq!(line.unwrap(), "  Server for model 3 stopped");
    assert_eq!(
        taken[0].line,
        format!("POST {} HTTP/1.1", paths::SERVERS_STOP_PATH)
    );
    assert_eq!(taken[0].body["model_id"], 3);

    let missing = serde_json::json!({ "error": "server not found: 3" });
    let (line, _) = against(vec![(404, missing.to_string())], |handle| async move {
        unload_on(&handle, 3).await
    })
    .await;
    let err = format!("{:#}", line.expect_err("not loaded"));
    assert!(err.contains("server not found: 3"), "{err}");
}
