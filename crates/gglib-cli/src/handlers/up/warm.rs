//! Step 5: prove the endpoint works, then say how to connect to it.
//!
//! This runs as a task alongside the proxy, because `start_proxy_standalone`
//! does not return until shutdown. It talks to the endpoint over HTTP like any
//! other client rather than reaching into the runtime: the point is to
//! exercise the router, the contention gate, model resolution and the forward
//! pipeline, so that "the endpoint works" is demonstrated rather than assumed.
//!
//! It is also what makes the launch narration part of `gglib up`'s output —
//! an unpinned proxy loads nothing until a request arrives, so without a first
//! request the user's evidence that anything happened is a bound socket.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde_json::json;

use super::{sgr, use_color};
use crate::presentation::style::{BOLD, DIM, RESET, SUCCESS, WARNING, make_spinner};

/// How long to keep polling for the listener before giving up on it.
const BIND_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the first request may take.
///
/// Generous on purpose: this covers reading tens of gigabytes of weights off
/// disk into VRAM on a cold page cache. Concurrent requests queue behind the
/// same startup rather than being refused, so waiting is the correct behaviour.
const LOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// Wait for the endpoint, send one real request through it, then print the
/// client configuration.
///
/// Never returns an error: a failed warm-up leaves a perfectly usable proxy,
/// and tearing that down over a slow first load would be a worse outcome than
/// the cold start it was trying to avoid.
pub(super) async fn run(port: u16, model: String) {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let client = reqwest::Client::new();

    if !wait_for_bind(&client, addr).await {
        // The proxy prints its own bind failure; adding a second one here
        // would just be noise.
        return;
    }

    let spinner = make_spinner();
    spinner.set_message(format!("Loading {model}..."));
    let started = Instant::now();
    let outcome = warm_request(&client, addr, &model).await;
    spinner.finish_and_clear();

    println!();
    match outcome {
        Ok(()) => println!(
            "  {}\u{2713}{} endpoint answered in {:.1}s",
            sgr(SUCCESS),
            sgr(RESET),
            started.elapsed().as_secs_f64()
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

    for line in render_client_config(addr, &model, use_color()) {
        println!("{line}");
    }
}

/// Poll `/health` until the listener answers.
///
/// Polling rather than being signalled: the supervisor's bind happens inside
/// `start_proxy_standalone`, which is busy blocking until shutdown and has no
/// readiness channel to offer.
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
) -> Result<(), String> {
    let response = client
        .post(format!("http://{addr}/v1/chat/completions"))
        .timeout(LOAD_TIMEOUT)
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

/// Render the connection details, as data.
///
/// Split from printing for the same reason the launch narration is: the
/// content is the deliverable — a user copies these values into a settings
/// dialog — so it deserves assertions rather than an eyeball over terminal
/// output.
///
/// Copilot gets its own line because its BYOK provider cannot switch models
/// through `/v1/models`, which is precisely what `gglib serve` exists for.
/// Sending someone there with the unpinned base URL would fail in a way that
/// looks like gglib being broken.
fn render_client_config(addr: SocketAddr, model: &str, color: bool) -> Vec<String> {
    let (bold, dim, reset) = if color {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };
    let base = format!("http://{addr}/v1");

    vec![
        String::new(),
        format!("  {bold}Connect your client{reset}"),
        String::new(),
        format!("    Cline / Roo / Continue   {dim}provider: OpenAI Compatible{reset}"),
        format!("      Base URL   {base}"),
        format!("      API Key    not-needed  {dim}(unused, but most clients demand one){reset}"),
        format!("      Model ID   {model}"),
        String::new(),
        format!("    Open WebUI               {dim}Settings -> Connections -> OpenAI API{reset}"),
        format!("      URL        {base}"),
        String::new(),
        format!("    VS Code Copilot (BYOK) needs one fixed model, so use:"),
        format!("      gglib serve {model}"),
        String::new(),
        format!("    Check it from another shell:"),
        format!("      curl {base}/models"),
        String::new(),
        format!("  Dashboard  http://{addr}/v1/proxy/status"),
        String::new(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr() -> SocketAddr {
        ([127, 0, 0, 1], 8080).into()
    }

    /// The three values a user actually retypes. If any goes missing the
    /// command has failed at its whole purpose.
    #[test]
    fn the_pasteable_values_are_all_present() {
        let lines = render_client_config(addr(), "qwen3-30b-a3b", false);
        let text = lines.join("\n");
        assert!(text.contains("http://127.0.0.1:8080/v1"));
        assert!(text.contains("qwen3-30b-a3b"));
        assert!(text.contains("not-needed"));
    }

    /// Copilot cannot use the unpinned endpoint; the pinned alternative has to
    /// be spelled out with the model already filled in.
    #[test]
    fn copilot_is_pointed_at_the_pinned_command() {
        let lines = render_client_config(addr(), "qwen3-30b-a3b", false);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("gglib serve qwen3-30b-a3b"))
        );
    }

    #[test]
    fn a_verification_command_is_offered() {
        let lines = render_client_config(addr(), "m", false);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("curl") && l.contains("/models"))
        );
    }

    #[test]
    fn the_dashboard_url_is_included() {
        let lines = render_client_config(addr(), "m", false);
        assert!(lines.iter().any(|l| l.contains("/v1/proxy/status")));
    }

    /// Piped or captured output must be clean — this block is the one most
    /// likely to be copied out of a terminal into a chat or an issue.
    #[test]
    fn no_ansi_escapes_when_color_is_off() {
        let lines = render_client_config(addr(), "m", false);
        assert!(lines.iter().all(|l| !l.contains('\u{1b}')));
    }

    #[test]
    fn ansi_escapes_appear_only_when_color_is_on() {
        let lines = render_client_config(addr(), "m", true);
        assert!(lines.iter().any(|l| l.contains(DIM)));
        assert!(lines.iter().any(|l| l.contains(BOLD)));
    }

    /// The address is not hardcoded — `--port` has to reach the printed URLs.
    #[test]
    fn a_non_default_port_reaches_every_url() {
        let lines = render_client_config(([127, 0, 0, 1], 9999).into(), "m", false);
        let text = lines.join("\n");
        assert!(text.contains("http://127.0.0.1:9999/v1"));
        assert!(!text.contains("8080"));
    }
}
