//! Step 5: prove the endpoint works, then say how to connect to it and what
//! to run next.
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

use gglib_core::secret::{SettingSecret, held_line};
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
pub(super) async fn run(port: u16, model: String, model_id: i64, api_key: Option<String>) {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let client = gglib_proxy::loopback::client();

    if !wait_for_bind(&client, addr).await {
        // The proxy prints its own bind failure; adding a second one here
        // would just be noise.
        return;
    }

    let spinner = make_spinner();
    spinner.set_message(format!("Loading {model}..."));
    let started = Instant::now();
    let outcome = warm_request(&client, addr, &model, api_key.as_deref()).await;
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

    for line in render_client_config(addr, &model, model_id, api_key.is_some(), use_color()) {
        println!("{line}");
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
///
/// Told whether the endpoint has a key, never the key, so it cannot print
/// one (ADR 0027): this block is the one most likely to be pasted into a chat
/// or an issue. The `curl` line reads the key through the reveal command.
fn render_client_config(
    addr: SocketAddr,
    model: &str,
    model_id: i64,
    has_key: bool,
    color: bool,
) -> Vec<String> {
    let (bold, dim, reset) = if color {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };
    let base = format!("http://{addr}/v1");

    // "not-needed" is the honest answer only for an unauthenticated
    // endpoint. A key is named, with the command that prints it.
    let secret = SettingSecret::ProxyApiKey;
    let (key_line, curl_line) = if has_key {
        (
            format!("      API Key    {}", held_line(secret)),
            format!(
                "      curl -H \"Authorization: Bearer $({})\" {base}/models",
                secret.reveal_command()
            ),
        )
    } else {
        (
            format!(
                "      API Key    not-needed  {dim}(unused, but most clients demand one){reset}"
            ),
            format!("      curl {base}/models"),
        )
    };

    vec![
        String::new(),
        format!("  {bold}Connect your client{reset}"),
        String::new(),
        format!("    Cline / Roo / Continue   {dim}provider: OpenAI Compatible{reset}"),
        format!("      Base URL   {base}"),
        key_line,
        format!("      Model ID   {model}"),
        String::new(),
        format!("    Open WebUI               {dim}Settings -> Connections -> OpenAI API{reset}"),
        format!("      URL        {base}"),
        String::new(),
        format!("    VS Code Copilot (BYOK) needs one fixed model, so use:"),
        format!("      gglib serve {model_id}"),
        String::new(),
        format!("    Check it from another shell:"),
        curl_line,
        String::new(),
        format!("  Dashboard  http://{addr}/v1/proxy/status"),
        String::new(),
    ]
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
    use gglib_core::settings::DEFAULT_PROXY_PORT;

    use super::*;

    fn addr() -> SocketAddr {
        ([127, 0, 0, 1], DEFAULT_PROXY_PORT).into()
    }

    /// The three values a user actually retypes. If any goes missing the
    /// command has failed at its whole purpose.
    #[test]
    fn the_pasteable_values_are_all_present() {
        let lines = render_client_config(addr(), "qwen3-30b-a3b", 7, false, false);
        let text = lines.join("\n");
        assert!(text.contains(&format!("http://127.0.0.1:{DEFAULT_PROXY_PORT}/v1")));
        assert!(text.contains("qwen3-30b-a3b"));
        assert!(text.contains("not-needed"));
    }

    /// Copilot cannot use the unpinned endpoint; the pinned alternative has to
    /// be spelled out with the model already filled in, by id: a name such as
    /// "Qwen3 8B" would reach `serve` as two arguments.
    #[test]
    fn copilot_is_pointed_at_the_pinned_command() {
        let lines = render_client_config(addr(), "Qwen3 8B", 7, false, false);
        assert!(lines.iter().any(|l| l.ends_with("gglib serve 7")));
        assert!(!lines.iter().any(|l| l.contains("gglib serve Qwen3")));
    }

    #[test]
    fn a_verification_command_is_offered() {
        let lines = render_client_config(addr(), "m", 1, false, false);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("curl") && l.contains("/models"))
        );
    }

    #[test]
    fn the_dashboard_url_is_included() {
        let lines = render_client_config(addr(), "m", 1, false, false);
        assert!(lines.iter().any(|l| l.contains("/v1/proxy/status")));
    }

    /// Piped or captured output must be clean — this block is the one most
    /// likely to be copied out of a terminal into a chat or an issue.
    #[test]
    fn no_ansi_escapes_when_color_is_off() {
        let lines = render_client_config(addr(), "m", 1, false, false);
        assert!(lines.iter().all(|l| !l.contains('\u{1b}')));
    }

    #[test]
    fn ansi_escapes_appear_only_when_color_is_on() {
        let lines = render_client_config(addr(), "m", 1, false, true);
        assert!(lines.iter().any(|l| l.contains(DIM)));
        assert!(lines.iter().any(|l| l.contains(BOLD)));
    }

    /// The address is not hardcoded — `--port` has to reach the printed URLs.
    #[test]
    fn a_non_default_port_reaches_every_url() {
        let lines = render_client_config(([127, 0, 0, 1], 9999).into(), "m", 1, false, false);
        let text = lines.join("\n");
        assert!(text.contains("http://127.0.0.1:9999/v1"));
        assert!(!text.contains(&DEFAULT_PROXY_PORT.to_string()));
    }

    /// A keyed endpoint is said to have a key, and the `curl` line reads it
    /// through the reveal, which then runs in the reader's own shell.
    #[test]
    fn a_key_is_named_and_read_through_the_reveal() {
        let text = render_client_config(addr(), "m", 1, true, false).join("\n");

        assert!(
            text.contains("API Key    (set, not shown)"),
            "the key is said to be held: {text}"
        );
        assert!(
            text.contains(
                "curl -H \"Authorization: Bearer $(gglib config settings reveal proxy-api-key)\""
            ),
            "{text}"
        );
        assert!(!text.contains("not-needed"), "{text}");
    }

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
