//! Step 5: prove the endpoint works, then say how to connect to it (the
//! client block, `client_block`) and what to run next.
//!
//! The proxy runs on the daemon, so this talks to the endpoint over HTTP like
//! any other client rather than reaching into a runtime: the point is to
//! exercise the router, admission control, model resolution and the forward
//! pipeline, so that "the endpoint works" is demonstrated rather than assumed.
//!
//! It is also what makes the launch narration part of `gglib up`'s output —
//! an unpinned proxy loads nothing until a request arrives, so without a first
//! request the user's evidence that anything happened is a bound socket.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde_json::json;

use super::client_block::{NO_ADDRESS, client_facts, render_client_config};
use super::{sgr, use_color};
use crate::handlers::inference::proxy::StartedProxy;
use crate::presentation::style::{BOLD, DIM, RESET, SUCCESS, WARNING, make_spinner};

/// How long to keep polling for the listener before giving up on it.
const BIND_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the first request may take.
///
/// Generous on purpose: this covers reading tens of gigabytes of weights off
/// disk into VRAM on a cold page cache. Concurrent requests queue behind the
/// same startup rather than being refused, so waiting is the correct behaviour.
const LOAD_TIMEOUT: Duration = Duration::from_mins(10);

/// Wait for the endpoint, send one real request through it, then print the
/// client configuration.
///
/// Never returns an error: a failed warm-up leaves a perfectly usable proxy,
/// and tearing that down over a slow first load would be a worse outcome than
/// the cold start it was trying to avoid.
///
/// `model` is what a client sends, the id the endpoint lists it under;
/// `model_id` is what a printed command takes: a name is often several words.
pub(super) async fn run(
    started: &StartedProxy,
    model: String,
    model_id: i64,
    api_key: Option<String>,
) {
    let addr: SocketAddr = ([127, 0, 0, 1], started.port).into();
    let client = gglib_proxy::loopback::client();

    if !wait_for_bind(&client, addr).await {
        // The proxy prints its own bind failure; adding a second one here
        // would just be noise.
        return;
    }

    let spinner = make_spinner();
    spinner.set_message(format!("Loading {model}..."));
    let begun = Instant::now();
    let outcome = warm_request(&client, addr, &model, api_key.as_deref()).await;
    spinner.finish_and_clear();

    println!();
    match outcome {
        Ok(()) => println!(
            "  {}\u{2713}{} endpoint answered in {:.1}s",
            sgr(SUCCESS),
            sgr(RESET),
            begun.elapsed().as_secs_f64()
        ),
        Err(e) => {
            println!(
                "  {}!{} the first request did not complete: {e}",
                sgr(WARNING),
                sgr(RESET)
            );
            println!(
                "  {}the endpoint is still running \u{2014} try it yourself below{}",
                sgr(DIM),
                sgr(RESET)
            );
        }
    }

    // The daemon said where a client reaches the endpoint and whether a
    // request needs the key; the block prints its answer and works neither
    // out for itself. A daemon that named no address is older than this
    // CLI and is told so; one that named the address and could not read
    // its settings to say of the key is printed as it answered.
    match client_facts(&started.status) {
        Some((base_url, key)) => {
            for line in render_client_config(base_url, &model, model_id, key, use_color()) {
                println!("{line}");
            }
        }
        None => println!("{NO_ADDRESS}"),
    }
}

/// Poll `/health` until the listener answers.
///
/// Polling rather than being signalled: the bind happens inside the daemon,
/// which offers no readiness channel to this process.
async fn wait_for_bind(client: &reqwest::Client, addr: SocketAddr) -> bool {
    let deadline = Instant::now() + BIND_TIMEOUT;
    let url = format!("http://{addr}/health");
    while Instant::now() < deadline {
        if client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

/// One minimal completion — enough to force the model to load, short enough
/// that generation time is noise next to load time.
async fn warm_request(
    client: &reqwest::Client,
    addr: SocketAddr,
    model: &str,
    api_key: Option<&str>,
) -> Result<(), String> {
    let mut request = client
        .post(format!("http://{addr}/v1/chat/completions"))
        .timeout(LOAD_TIMEOUT);
    // `up` never passes `--api-key`, but the stored setting still applies to
    // the proxy it just started — so this probe has to authenticate like any
    // other client would.
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = request
        .json(&json!({
            "model": model,
            "messages": [{ "role": "user", "content": "hi" }],
            "max_tokens": 1,
            "stream": false,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(format!("{status}: {}", body.trim()))
    }
}

/// What to run once `up` is done, as data: the endpoint keeps serving on the
/// daemon, so the list ends with how to watch it and how to stop it. The model
/// is named by its id, which a shell passes as one argument.
pub(super) fn render_next_steps(port: u16, model_id: i64, color: bool) -> Vec<String> {
    let (bold, dim, reset) = if color {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };
    let watch = format!("gglib proxy dashboard --port {port}");
    vec![
        format!("  {bold}Ready.{reset} The endpoint keeps running in the background."),
        String::new(),
        format!(
            "    {:<36}{dim}chat in this terminal{reset}",
            format!("gglib chat {model_id}")
        ),
        format!(
            "    {:<36}{dim}open the chat page in your browser{reset}",
            "gglib web"
        ),
        format!("    {watch:<36}{dim}watch requests as they arrive{reset}"),
        format!(
            "    {:<36}{dim}stop the endpoint{reset}",
            "gglib proxy stop"
        ),
        String::new(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `up` ends here rather than in the dashboard, so the list has to name
    /// the chat, the web page, and how to watch and stop what is still
    /// running, with this run's model and port filled in.
    #[test]
    fn the_next_steps_name_this_runs_model_and_port() {
        let text = render_next_steps(8123, 7, false).join("\n");
        for command in [
            "gglib chat 7 ",
            "gglib web",
            "gglib proxy dashboard --port 8123",
            "gglib proxy stop",
        ] {
            assert!(text.contains(command), "missing {command:?} in:\n{text}");
        }
        assert!(!text.contains('\x1b'));
    }
}
