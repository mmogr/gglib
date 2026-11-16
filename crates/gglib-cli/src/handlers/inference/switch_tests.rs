//! Tests for [`super`]: a start that finds the endpoint on otherwise asks
//! before it switches, `-y` switches without asking, a run with nobody to
//! ask changes nothing and names `-y`, and an endpoint already as asked is
//! said so and asks nothing. Each runs `start_on` against a stand-in daemon
//! that answers as the real one does.

use std::sync::{Arc, Mutex};

use super::*;
use crate::daemon_client::stand_in::against;
use crate::daemon_client::{StartProxyBody, paths};
use crate::handlers::inference::proxy::start_on;

/// The daemon's refusal of a pinned start with every model being served.
fn differs() -> (u16, String) {
    (
        409,
        serde_json::json!({
            "error": "The endpoint is on at port 8080, serving every model. ...",
            "status": 409,
            "type": paths::ENDPOINT_DIFFERS_TYPE,
            "metadata": {
                "now": "on at port 8080, serving every model",
                "action": "serve only Qwen3-4B",
            },
        })
        .to_string(),
    )
}

/// The daemon's answer to a start, with what it did.
fn on(outcome: &str, pinned: Option<&str>) -> (u16, String) {
    (
        200,
        serde_json::json!({
            "running": true,
            "state": if pinned.is_some() { "pinned" } else { "on" },
            "port": 8080,
            "base_url": "http://127.0.0.1:8080/v1",
            "key": "none_needed",
            "pinned_model": pinned,
            "pinned_to": pinned.map(|m| format!("Pinned to {m}")),
            "pinned_model_id": pinned.map(|_| 3),
            "outcome": outcome,
        })
        .to_string(),
    )
}

/// A consent that answers `answer` to every question and keeps each.
fn answering(answer: bool, yes: bool, terminal: bool) -> (Consent, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&asked);
    let consent = Consent {
        yes,
        terminal,
        command: "gglib serve Qwen3-4B".to_owned(),
        ask: Box::new(move |question| {
            kept.lock().unwrap().push(question.to_owned());
            Ok(answer)
        }),
    };
    (consent, asked)
}

/// In a terminal, the question names what runs and what switching does; a
/// yes sends the same start again with `replace`.
#[tokio::test]
async fn in_a_terminal_it_asks_and_a_yes_switches() {
    let (mut consent, asked) = answering(true, false, true);
    let (started, taken) = against(
        vec![differs(), on("replaced", Some("Qwen3-4B"))],
        |handle| async move {
            start_on(
                &handle,
                &StartProxyBody::default(),
                &gglib_core::Settings::default(),
                &mut consent,
            )
            .await
        },
    )
    .await;

    assert_eq!(started.unwrap().port, 8080);
    assert_eq!(
        *asked.lock().unwrap(),
        [
            "  The endpoint is on at port 8080, serving every model. Serve only Qwen3-4B? \
             Clients connected now are cut off for a moment."
        ]
    );
    assert_eq!(taken.len(), 2);
    assert_eq!(taken[0].body["replace"], false);
    assert_eq!(taken[1].body["replace"], true);
}

/// A no sends nothing more, and the command fails with the endpoint as it
/// is, so a script after it does not go on.
#[tokio::test]
async fn a_no_leaves_everything_as_it_was() {
    let (mut consent, asked) = answering(false, false, true);
    let (started, taken) = against(
        vec![differs(), on("replaced", Some("Qwen3-4B"))],
        |handle| async move {
            start_on(
                &handle,
                &StartProxyBody::default(),
                &gglib_core::Settings::default(),
                &mut consent,
            )
            .await
        },
    )
    .await;

    assert_eq!(
        format!("{:#}", started.expect_err("a no is not a start")),
        "Nothing changed: the endpoint is still on at port 8080, serving every model."
    );
    assert_eq!(asked.lock().unwrap().len(), 1);
    assert_eq!(taken.len(), 1, "nothing was sent after the no");
}

/// `-y` switches without a question, in a terminal or not.
#[tokio::test]
async fn yes_switches_without_asking() {
    let (mut consent, asked) = answering(false, true, false);
    let (started, taken) = against(
        vec![differs(), on("replaced", Some("Qwen3-4B"))],
        |handle| async move {
            start_on(
                &handle,
                &StartProxyBody::default(),
                &gglib_core::Settings::default(),
                &mut consent,
            )
            .await
        },
    )
    .await;

    assert!(started.is_ok());
    assert!(asked.lock().unwrap().is_empty(), "-y asks nothing");
    assert_eq!(taken.len(), 2);
    assert_eq!(taken[1].body["replace"], true);
}

/// With nobody to ask and no `-y`, nothing is switched, and the refusal is
/// one sentence that names the command to run, as typed, with `-y`.
#[tokio::test]
async fn a_script_without_yes_is_refused_and_switches_nothing() {
    let (mut consent, asked) = answering(true, false, false);
    let (started, taken) = against(
        vec![differs(), on("replaced", Some("Qwen3-4B"))],
        |handle| async move {
            start_on(
                &handle,
                &StartProxyBody::default(),
                &gglib_core::Settings::default(),
                &mut consent,
            )
            .await
        },
    )
    .await;

    let refused = format!("{:#}", started.expect_err("nobody to ask"));
    assert_eq!(
        refused,
        "The endpoint is on at port 8080, serving every model, so nothing was changed: \
         to serve only Qwen3-4B, run `gglib serve Qwen3-4B -y`."
    );
    assert!(asked.lock().unwrap().is_empty());
    assert_eq!(taken.len(), 1, "nothing was sent after the refusal");
}

/// An endpoint already as asked asks nothing, and is said so.
#[tokio::test]
async fn the_same_endpoint_asks_nothing() {
    let (mut consent, asked) = answering(false, false, true);
    let (started, taken) = against(
        vec![on("unchanged", Some("Qwen3-4B"))],
        |handle| async move {
            start_on(
                &handle,
                &StartProxyBody::default(),
                &gglib_core::Settings::default(),
                &mut consent,
            )
            .await
        },
    )
    .await;

    assert!(started.is_ok());
    assert!(asked.lock().unwrap().is_empty());
    assert_eq!(taken.len(), 1);
    assert_eq!(
        unchanged(8080, Some("Qwen3-4B")),
        "  The endpoint is already on at port 8080, serving only Qwen3-4B, as asked."
    );
}

/// `gglib proxy` with flags the running endpoint was not started with is
/// asked about by name, as the daemon words the difference; going back to
/// every model is asked the same way.
#[test]
fn the_question_names_the_difference() {
    let port = EndpointChange {
        now: "on at port 8080, serving every model".into(),
        action: "restart it with port 9929 (now 8080) and different sampling".into(),
        moves: true,
    };
    assert_eq!(
        question(&port),
        "  The endpoint is on at port 8080, serving every model. Restart it with port 9929 \
         (now 8080) and different sampling? Clients connected now are cut off until they are \
         pointed at the new address."
    );
    let back = EndpointChange {
        now: "on at port 8080, serving only Qwen3-4B".into(),
        action: "serve every model again".into(),
        moves: false,
    };
    assert_eq!(
        question(&back),
        "  The endpoint is on at port 8080, serving only Qwen3-4B. Serve every model again? \
         Clients connected now are cut off for a moment."
    );
}

/// A command that left the endpoint serving one model closes with how to
/// serve every model again, in `gglib up`'s layout.
#[test]
fn the_way_back_names_gglib_proxy() {
    assert_eq!(
        way_back(),
        "    gglib proxy                         serve every model again"
    );
}

/// `gglib up` leaves an endpoint pinned to a model as it is: it asks
/// nothing, sends no `replace`, says so with the way back, and proves the
/// model the endpoint serves.
#[tokio::test]
async fn up_leaves_a_pinned_endpoint_as_it_is() {
    let pinned = on("unchanged", Some("Qwen3-4B"));
    let (started, taken) = against(
        vec![differs_pinned(), (200, status_of(&pinned.1))],
        |handle| async move {
            crate::handlers::inference::proxy::start_or_leave(
                &handle,
                &StartProxyBody::default(),
                &gglib_core::Settings::default(),
                "gglib proxy",
            )
            .await
        },
    )
    .await;

    let started = started.expect("left as it is");
    assert_eq!(started.port, 8080);
    assert_eq!(
        started.pinned_model_id,
        Some(3),
        "the model it serves is proved"
    );
    assert_eq!(taken.len(), 2, "the start, then the status; no switch");
    assert_eq!(taken[0].body["replace"], false);
    assert_eq!(
        taken[1].line,
        format!("GET {} HTTP/1.1", paths::PROXY_STATUS_PATH)
    );
    assert_eq!(
        left(
            &EndpointChange {
                now: "on at port 8080, serving only Qwen3-4B".into(),
                action: "serve every model again".into(),
                moves: false,
            },
            "gglib proxy"
        ),
        [
            "  The endpoint is on at port 8080, serving only Qwen3-4B, so gglib up left it as it \
             is and did not serve every model again.",
            "  To do that, run: gglib proxy",
        ]
    );
}

/// The daemon's refusal of an unpinned start with one model being served.
fn differs_pinned() -> (u16, String) {
    (
        409,
        serde_json::json!({
            "error": "The endpoint is on at port 8080, serving only Qwen3-4B, so it was left as it is.",
            "status": 409,
            "type": paths::ENDPOINT_DIFFERS_TYPE,
            "metadata": {
                "now": "on at port 8080, serving only Qwen3-4B",
                "action": "serve every model again",
            },
        })
        .to_string(),
    )
}

/// A status answer, without what a start did.
fn status_of(answer: &str) -> String {
    let mut status: serde_json::Value = serde_json::from_str(answer).expect("JSON");
    status.as_object_mut().expect("an object").remove("outcome");
    status.to_string()
}

/// The question is asked only of somebody who both reads it and types the
/// answer: a question sent to a log file waits on nobody.
#[test]
fn somebody_is_asked_only_at_a_terminal_both_ways() {
    assert!(somebody_to_ask(true, true));
    assert!(!somebody_to_ask(true, false), "stderr to a file");
    assert!(!somebody_to_ask(false, true), "stdin from a pipe");
}

/// The command the refusal names is the one typed, as a shell takes it.
#[test]
fn the_command_named_is_the_one_typed() {
    let typed = ["serve", "Qwen3 4B", "--temperature", "0.5", "it's"].map(str::to_owned);
    assert_eq!(
        command_line(typed),
        "gglib serve 'Qwen3 4B' --temperature 0.5 'it'\\''s'"
    );
}
