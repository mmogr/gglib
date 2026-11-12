//! `gglib model explain` beside a daemon: what a Load would unload is the
//! daemon's to say, from what it has loaded, and a stand-in on a loopback
//! port answers in its place. With no daemon that serves this library, the
//! command plans for itself over nothing loaded.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_core::domain::{Model, Unload, UnloadReason};
use gglib_test_http::blocking;

use super::*;
use crate::daemon_client::handle_tests::{daemon_health, nobody};
use crate::daemon_client::library_changes::tests::{TOKEN, beside};
use crate::handlers::model::one_shot_model_ops;
use crate::handlers::model::test_library::library;
use crate::presentation::launch_display;

/// The first line of each request a stand-in was sent.
type Asked = Arc<Mutex<Vec<String>>>;

/// A stand-in daemon on a loopback port: `/health` as this build's daemon
/// answers it, and every other request with `status` and `body`.
fn daemon(status: &'static str, body: String) -> (u16, Asked) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let asked = Asked::default();
    let seen = Arc::clone(&asked);
    let health = daemon_health();
    std::thread::spawn(move || {
        loop {
            let Ok(taken) = blocking::incoming(&listener) else {
                continue;
            };
            let Ok((request, mut stream)) = taken.request() else {
                seen.lock().unwrap().push(String::new());
                continue;
            };
            let line = request.line().to_owned();
            let (status, body) = if line.starts_with("GET /health ") {
                ("200 OK", &health)
            } else {
                (status, &body)
            };
            seen.lock().unwrap().push(line);
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, asked)
}

/// The requests a stand-in was sent, but its `/health` probes.
fn asked_past_health(asked: &Asked) -> Vec<String> {
    let lines = asked.lock().unwrap();
    lines
        .iter()
        .filter(|line| !line.starts_with("GET /health "))
        .cloned()
        .collect()
}

/// The explanation the command's own `ModelOps` gives, with nothing loaded.
async fn local(ctx: &CliContext, model: &Model) -> LaunchExplanationDto {
    explain(&one_shot_model_ops(ctx), model.id, None)
        .await
        .expect("explained here")
}

/// The explanation `gglib model explain` reads for `model` with the daemon
/// on `port` and `token` left under this data root.
async fn explained(
    ctx: &CliContext,
    model: &Model,
    port: u16,
    token: Option<&str>,
    profile: Option<&str>,
) -> Result<LaunchExplanationDto> {
    let ops = one_shot_model_ops(ctx);
    beside(port, token, explanation(&ops, model.id, profile)).await
}

/// With a model loaded that the launch would unload, the daemon's plan says
/// so and the command prints the sentence the GUI's Load palette shows
/// (`contracts/models/unload_reason.json`). The command's own plan knows of
/// nothing loaded, so the line can only have come from the daemon.
#[tokio::test]
async fn a_load_that_would_unload_a_loaded_model_says_which_and_why() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let mut answer = local(&ctx, &model).await;
    assert_eq!(
        answer.plan.unloads, None,
        "the one-shot plan unloads nothing"
    );
    answer.plan.unloads = Some(Unload {
        model: "Qwen3-30B-A3B".to_owned(),
        reason: UnloadReason::WouldNotFitBeside,
    });
    let (port, asked) = daemon("200 OK", serde_json::to_string(&answer).unwrap());

    let plan = explained(&ctx, &model, port, Some(TOKEN), None)
        .await
        .unwrap()
        .plan;

    let text = launch_display::launch_text(&model.name, model.id, &plan);
    assert!(
        text.contains("\n  Unloads Qwen3-30B-A3B first: it would not fit beside it.\n"),
        "{text}"
    );
    let route = format!("GET /api/models/{}/explain HTTP/1.1", model.id);
    assert_eq!(asked_past_health(&asked), [route]);
}

/// With nothing loaded, or room beside what is, the daemon's plan unloads
/// nothing and no line says otherwise. A named profile is sent along with
/// every byte a query would read as something else encoded.
#[tokio::test]
async fn a_load_that_unloads_nothing_has_no_unload_line() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let answer = serde_json::to_string(&local(&ctx, &model).await).unwrap();
    let (port, asked) = daemon("200 OK", answer);

    let plan = explained(&ctx, &model, port, Some(TOKEN), Some("a+b&c"))
        .await
        .unwrap()
        .plan;

    let text = launch_display::launch_text(&model.name, model.id, &plan);
    assert!(!text.contains("Unload"), "{text}");
    let route = format!(
        "GET /api/models/{}/explain?profile=a%2Bb%26c HTTP/1.1",
        model.id
    );
    assert_eq!(asked_past_health(&asked), [route]);
}

/// A profile the daemon has not configured is its 400, and the command's
/// error is the daemon's sentence alone, as a one-shot refusal is.
#[tokio::test]
async fn a_profile_the_daemon_refuses_is_its_sentence_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let said = "no profile named 'codign'; configured profiles are: coding, chat";
    let body = serde_json::json!({ "error": said }).to_string();
    let (port, _) = daemon("400 Bad Request", body);

    let refused = explained(&ctx, &model, port, Some(TOKEN), Some("codign")).await;

    assert_eq!(refused.expect_err("refused").to_string(), said);
}

/// This library's daemon failing is the command failing: only a 401 (a
/// daemon serving another library) falls back to the local plan, which
/// would say nothing of what is loaded.
#[tokio::test]
async fn a_daemon_that_fails_fails_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let body = serde_json::json!({ "error": "the planner panicked" }).to_string();
    let (port, _) = daemon("500 Internal Server Error", body);

    let failed = explained(&ctx, &model, port, Some(TOKEN), None).await;

    assert!(failed.is_err(), "a 500 must not read as a plan");
}

/// No daemon serves this library, so nothing is loaded: the command plans
/// for itself. A token left by a daemon that has since stopped finds nothing
/// on the port; a daemon on the port with no token under this data root, or
/// one that refuses the token here (401), serves another library.
#[tokio::test]
async fn with_no_daemon_of_this_library_the_command_plans_for_itself() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let here = local(&ctx, &model).await;
    let (other, asked_other) = daemon("200 OK", "{}".to_owned());
    let refusal = serde_json::json!({ "error": "wrong token" }).to_string();
    let (refusing, asked_refusing) = daemon("401 Unauthorized", refusal);

    for (port, token) in [
        (nobody(), None),
        (nobody(), Some(TOKEN)),
        (other, None),
        (refusing, Some(TOKEN)),
    ] {
        let explained = explained(&ctx, &model, port, token, None).await;
        let explained = explained.unwrap_or_else(|e| panic!("{port} {token:?}: {e:#}"));
        assert_eq!(explained, here, "{port} {token:?}");
    }
    assert!(asked_other.lock().unwrap().is_empty());
    let route = format!("GET /api/models/{}/explain HTTP/1.1", model.id);
    assert_eq!(asked_past_health(&asked_refusing), [route]);
}
