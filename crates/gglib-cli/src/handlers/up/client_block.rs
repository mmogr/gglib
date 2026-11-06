//! The client block `gglib up` ends with: where a client reaches the
//! endpoint and whether it needs a key, written from the daemon's answer and
//! from nothing else, and the lines a person pastes into Cline, `Open WebUI`
//! or a shell.

use gglib_core::domain::KeyState;
use gglib_core::secret::{SettingSecret, held_line};

use crate::daemon_client::ProxyStatusDto;
use crate::presentation::style::{BOLD, DIM, RESET};

/// What `up` says when the daemon's answer named no address: a listening
/// daemon of this build always does, so it is an older one.
pub(super) const NO_ADDRESS: &str = "  The daemon did not say where the endpoint is reached: it is older \
                          than this CLI. Update it, then `gglib daemon status`.";

/// The facts the client block is written from, as the daemon's answer
/// carries them: the address, without which there is no block, and the key
/// requirement, which the daemon leaves out when it could not read its
/// settings to say. Nothing here stands in for either.
pub(super) fn client_facts(status: &ProxyStatusDto) -> Option<(&str, Option<KeyState>)> {
    Some((status.base_url.as_deref()?, status.key))
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
/// `base` is the daemon's `base_url`, printed as it came. Told whether the
/// endpoint needs a key, never the key, so it cannot print one (ADR 0027):
/// this block is the one most likely to be pasted into a chat or an issue.
/// The `curl` line reads the key through the reveal command. `None` is a
/// daemon that could not read its settings to say, and is said as that.
pub(super) fn render_client_config(
    base: &str,
    model: &str,
    model_id: i64,
    key: Option<KeyState>,
    color: bool,
) -> Vec<String> {
    let (bold, dim, reset) = if color {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };

    // A key that is needed is named, with the command that prints it; one
    // that is not is said in the word every surface uses; one the daemon
    // could not read its settings to say of is said as unknown.
    let secret = SettingSecret::ProxyApiKey;
    let (key_line, curl_line) = match key {
        Some(KeyState::Required) => (
            format!("      API Key    {}", held_line(secret)),
            format!(
                "      curl -H \"Authorization: Bearer $({})\" {base}/models",
                secret.reveal_command()
            ),
        ),
        Some(word @ KeyState::NoneNeeded) => (
            format!("      API Key    {word}  {dim}(unused, but most clients demand one){reset}"),
            format!("      curl {base}/models"),
        ),
        None => (
            format!(
                "      API Key    unknown  {dim}(the daemon could not read its settings to say; \
                 `gglib daemon status` says once it can){reset}"
            ),
            format!("      curl {base}/models"),
        ),
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
        format!("  Dashboard  {base}/proxy/status"),
        String::new(),
    ]
}

#[cfg(test)]
mod tests {
    use gglib_core::settings::DEFAULT_PROXY_PORT;

    use super::*;

    fn base() -> String {
        format!("http://127.0.0.1:{DEFAULT_PROXY_PORT}/v1")
    }

    /// The three values a user actually retypes. If any goes missing the
    /// command has failed at its whole purpose.
    #[test]
    fn the_pasteable_values_are_all_present() {
        let lines = render_client_config(
            &base(),
            "qwen3-30b-a3b",
            7,
            Some(KeyState::NoneNeeded),
            false,
        );
        let text = lines.join("\n");
        assert!(text.contains(&format!("http://127.0.0.1:{DEFAULT_PROXY_PORT}/v1")));
        assert!(text.contains("qwen3-30b-a3b"));
        assert!(text.contains("API Key    None needed"));
    }

    /// Copilot cannot use the unpinned endpoint; the pinned alternative has to
    /// be spelled out with the model already filled in, by id: a name such as
    /// "Qwen3 8B" would reach `serve` as two arguments.
    #[test]
    fn copilot_is_pointed_at_the_pinned_command() {
        let lines = render_client_config(&base(), "Qwen3 8B", 7, Some(KeyState::NoneNeeded), false);
        assert!(lines.iter().any(|l| l.ends_with("gglib serve 7")));
        assert!(!lines.iter().any(|l| l.contains("gglib serve Qwen3")));
    }

    #[test]
    fn a_verification_command_is_offered() {
        let lines = render_client_config(&base(), "m", 1, Some(KeyState::NoneNeeded), false);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("curl") && l.contains("/models"))
        );
    }

    #[test]
    fn the_dashboard_url_is_included() {
        let lines = render_client_config(&base(), "m", 1, Some(KeyState::NoneNeeded), false);
        assert!(lines.iter().any(|l| l.contains("/v1/proxy/status")));
    }

    /// Piped or captured output must be clean — this block is the one most
    /// likely to be copied out of a terminal into a chat or an issue.
    #[test]
    fn no_ansi_escapes_when_color_is_off() {
        let lines = render_client_config(&base(), "m", 1, Some(KeyState::NoneNeeded), false);
        assert!(lines.iter().all(|l| !l.contains('\u{1b}')));
    }

    #[test]
    fn ansi_escapes_appear_only_when_color_is_on() {
        let lines = render_client_config(&base(), "m", 1, Some(KeyState::NoneNeeded), true);
        assert!(lines.iter().any(|l| l.contains(DIM)));
        assert!(lines.iter().any(|l| l.contains(BOLD)));
    }

    /// The address is not hardcoded — `--port` has to reach the printed URLs.
    #[test]
    fn a_non_default_port_reaches_every_url() {
        let lines = render_client_config(
            "http://127.0.0.1:9999/v1",
            "m",
            1,
            Some(KeyState::NoneNeeded),
            false,
        );
        let text = lines.join("\n");
        assert!(text.contains("http://127.0.0.1:9999/v1"));
        assert!(!text.contains(&DEFAULT_PROXY_PORT.to_string()));
    }

    /// A keyed endpoint is said to have a key, and the `curl` line reads it
    /// through the reveal, which then runs in the reader's own shell.
    #[test]
    fn a_key_is_named_and_read_through_the_reveal() {
        let text =
            render_client_config(&base(), "m", 1, Some(KeyState::Required), false).join("\n");

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
        assert!(!text.contains("None needed"), "{text}");
    }

    /// A daemon that could not read its settings to say of the key is
    /// printed as unknown, with a plain `curl`: no word is chosen for it.
    #[test]
    fn an_unknown_key_is_said_to_be_unknown() {
        let text = render_client_config(&base(), "m", 1, None, false).join("\n");
        assert!(text.contains("API Key    unknown"), "{text}");
        assert!(text.contains("      curl http://"), "{text}");
        assert!(
            !text.contains("None needed") && !text.contains("Bearer"),
            "{text}"
        );
    }

    /// The block is written from the daemon's answer and from nothing else:
    /// no address gives no block, and a missing key stays missing, so no
    /// address or key word can be worked out here.
    #[test]
    fn the_client_facts_are_the_daemons_or_none() {
        use gglib_core::domain::EndpointState;
        let full = ProxyStatusDto {
            running: true,
            state: EndpointState::On,
            port: Some(8080),
            base_url: Some("http://192.168.1.20:8080/v1".to_owned()),
            key: Some(KeyState::Required),
            pinned_model: None,
        };
        assert_eq!(
            client_facts(&full),
            Some(("http://192.168.1.20:8080/v1", Some(KeyState::Required)))
        );
        let no_url = ProxyStatusDto {
            base_url: None,
            ..full.clone()
        };
        assert_eq!(client_facts(&no_url), None);
        let no_key = ProxyStatusDto { key: None, ..full };
        assert_eq!(
            client_facts(&no_key),
            Some(("http://192.168.1.20:8080/v1", None))
        );
        assert!(NO_ADDRESS.contains("older than this CLI"));
    }
}
