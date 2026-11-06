//! The client block `gglib up` and `gglib proxy test` end with: where a
//! client reaches the endpoint, whether it needs a key, the id a client
//! sends, and a recipe per client, all as the daemon answered
//! (`GET /api/proxy/clients`), laid out in columns and printed. Nothing here
//! works a value out; the daemon wrote every one.

use gglib_core::domain::KeyState;
use gglib_core::secret::{SettingSecret, held_line};

use crate::daemon_client::{ClientRecipesDto, ProofOutcomeDto};
use crate::presentation::style::{BOLD, DIM, RESET, SUCCESS, WARNING};

/// The proof's one line: who answered and in how long, or why not.
#[must_use]
pub(crate) fn render_proof(outcome: Result<&ProofOutcomeDto, &str>, color: bool) -> Vec<String> {
    let (success, warning, dim, reset) = if color {
        (SUCCESS, WARNING, DIM, RESET)
    } else {
        ("", "", "", "")
    };
    match outcome {
        // A pinned endpoint answers with its pin whatever was asked for, so
        // the two are both said when they differ.
        Ok(proof) => {
            let asked = if proof.answered_by == proof.asked_for {
                String::new()
            } else {
                format!(" {dim}(asked for {}){reset}", proof.asked_for)
            };
            // Tenths of a second from the milliseconds, to the nearest.
            let tenths = (proof.elapsed_ms + 50) / 100;
            vec![format!(
                "  {success}\u{2713}{reset} answered in {}.{}s by {}{asked}",
                tenths / 10,
                tenths % 10,
                proof.answered_by
            )]
        }
        Err(reason) => vec![
            format!("  {warning}!{reset} the first request did not complete: {reason}"),
            format!("  {dim}the endpoint is still running \u{2014} try it yourself below{reset}"),
        ],
    }
}

/// Render the connection details, as data.
///
/// Split from printing for the same reason the launch narration is: the
/// content is the deliverable — a user copies these values into a settings
/// dialog — so it deserves assertions rather than an eyeball over terminal
/// output.
///
/// The recipes are the daemon's, printed in its order with its labels. Told
/// whether the endpoint needs a key, never the key, so this cannot print one
/// (ADR 0027): this block is the one most likely to be pasted into a chat or
/// an issue. The `curl` line reads the key through the reveal command when
/// one is needed.
#[must_use]
pub(crate) fn render_client_config(recipes: &ClientRecipesDto, color: bool) -> Vec<String> {
    let (bold, dim, reset) = if color {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };
    let base = recipes.base_url.as_str();
    let width = recipes
        .recipes
        .iter()
        .flat_map(|r| r.lines.iter().map(|l| l.label.len()))
        .max()
        .unwrap_or(0)
        .max(8);

    let mut out = vec![
        String::new(),
        format!(
            "  {bold}Connect your client{reset}  {dim}Model ID {} \u{b7} gglib id {}{reset}",
            recipes.model_id, recipes.gglib_id
        ),
        String::new(),
    ];
    for recipe in &recipes.recipes {
        out.push(format!(
            "    {:<24} {dim}{}{reset}",
            recipe.client, recipe.hint
        ));
        for line in &recipe.lines {
            // A key the endpoint checks is said as the stored secret, with
            // the command that prints it, in place of the daemon's placeholder.
            let value = if recipes.key == Some(KeyState::Required)
                && line.label.to_ascii_lowercase().contains("key")
            {
                held_line(SettingSecret::ProxyApiKey)
            } else {
                line.value.clone()
            };
            out.push(format!("      {:<width$} {value}", line.label));
        }
        out.push(String::new());
    }

    let curl = match recipes.key {
        Some(KeyState::Required) => format!(
            "      curl -H \"Authorization: Bearer $({})\" {base}/models",
            SettingSecret::ProxyApiKey.reveal_command()
        ),
        _ => format!("      curl {base}/models"),
    };
    out.push("    Check it from another shell:".to_owned());
    out.push(curl);
    out.push(String::new());
    out.push(format!("  Dashboard  {base}/proxy/status"));
    out.push(String::new());
    out
}

#[cfg(test)]
mod tests {
    use gglib_core::settings::DEFAULT_PROXY_PORT;

    use super::*;

    fn base() -> String {
        format!("http://127.0.0.1:{DEFAULT_PROXY_PORT}/v1")
    }

    /// The daemon's answer for one model, as `GET /api/proxy/clients` sends it.
    fn answer(model: &str, gglib_id: i64, key: Option<KeyState>) -> ClientRecipesDto {
        let key_value = match key {
            Some(KeyState::Required) => "the stored key (required)",
            _ => "gglib  (unused, but most clients demand one)",
        };
        serde_json::from_value(serde_json::json!({
            "base_url": base(),
            "key": key,
            "model_id": model,
            "gglib_id": gglib_id,
            "recipes": [
                { "client": "Cline / Roo / Continue", "hint": "provider: OpenAI Compatible", "lines": [
                    { "label": "Base URL", "value": base() },
                    { "label": "API Key", "value": key_value },
                    { "label": "Model ID", "value": model },
                ] },
                { "client": "Open WebUI", "hint": "Settings -> Connections -> OpenAI API", "lines": [
                    { "label": "URL", "value": base() },
                ] },
                { "client": "VS Code Copilot (BYOK)", "hint": "needs one fixed model, so serve only this one", "lines": [
                    { "label": "Run", "value": format!("gglib serve {gglib_id}") },
                ] },
            ],
        }))
        .expect("the daemon's shape reads")
    }

    /// The three values a user actually retypes. If any goes missing the
    /// command has failed at its whole purpose.
    #[test]
    fn the_pasteable_values_are_all_present() {
        let lines = render_client_config(
            &answer("qwen3-30b-a3b", 7, Some(KeyState::NoneNeeded)),
            false,
        );
        let text = lines.join("\n");
        assert!(text.contains(&format!("http://127.0.0.1:{DEFAULT_PROXY_PORT}/v1")));
        assert!(text.contains("qwen3-30b-a3b"));
        assert!(
            text.contains("Model ID qwen3-30b-a3b \u{b7} gglib id 7"),
            "{text}"
        );
        assert!(text.contains("API Key  gglib  (unused, but most clients demand one)"));
    }

    /// Copilot cannot use the unpinned endpoint; the pinned alternative is
    /// printed as the daemon sent it, with the model by id.
    #[test]
    fn copilot_is_pointed_at_the_pinned_command() {
        let lines = render_client_config(&answer("Qwen3 8B", 7, Some(KeyState::NoneNeeded)), false);
        assert!(
            lines.iter().any(|l| l.ends_with("gglib serve 7")),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("gglib serve Qwen3")));
    }

    /// A key the endpoint checks is never printed: its line names the stored
    /// secret and the command that reveals it, and the curl reads it through
    /// that command.
    #[test]
    fn a_required_key_is_named_and_read_through_the_reveal_command() {
        let lines = render_client_config(&answer("m", 1, Some(KeyState::Required)), false);
        let text = lines.join("\n");
        assert!(
            text.contains(&held_line(SettingSecret::ProxyApiKey)),
            "{text}"
        );
        assert!(
            !text.contains("the stored key (required)"),
            "the daemon's placeholder is replaced: {text}"
        );
        assert!(
            lines.iter().any(|l| l.contains("curl -H")
                && l.contains(SettingSecret::ProxyApiKey.reveal_command())),
            "{text}"
        );
    }

    #[test]
    fn a_verification_command_is_offered() {
        let lines = render_client_config(&answer("m", 1, Some(KeyState::NoneNeeded)), false);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("curl") && l.contains("/models"))
        );
    }

    #[test]
    fn the_dashboard_url_is_included() {
        let lines = render_client_config(&answer("m", 1, Some(KeyState::NoneNeeded)), false);
        assert!(lines.iter().any(|l| l.contains("/v1/proxy/status")));
    }

    /// Piped or captured output must be clean — this block is the one most
    /// likely to be copied out of a terminal into a chat or an issue.
    #[test]
    fn no_ansi_escapes_when_color_is_off() {
        let lines = render_client_config(&answer("m", 1, Some(KeyState::NoneNeeded)), false);
        assert!(lines.iter().all(|l| !l.contains('\u{1b}')));
    }

    #[test]
    fn ansi_escapes_appear_only_when_color_is_on() {
        let lines = render_client_config(&answer("m", 1, Some(KeyState::NoneNeeded)), true);
        assert!(lines.iter().any(|l| l.contains('\u{1b}')));
    }

    /// The proof's line says who answered and how long it took, from the
    /// daemon's answer; a refusal prints the daemon's words.
    #[test]
    fn the_proof_line_names_who_answered_and_the_time() {
        let proof = ProofOutcomeDto {
            answered_by: "qwen3-8b".to_owned(),
            asked_for: "Qwen3 8B".to_owned(),
            elapsed_ms: 1_400,
        };
        assert_eq!(
            render_proof(Ok(&proof), false),
            ["  \u{2713} answered in 1.4s by qwen3-8b (asked for Qwen3 8B)"]
        );
        let same = ProofOutcomeDto {
            asked_for: "qwen3-8b".to_owned(),
            ..proof
        };
        assert_eq!(
            render_proof(Ok(&same), false),
            ["  \u{2713} answered in 1.4s by qwen3-8b"]
        );
        let refused = render_proof(Err("503 Service Unavailable: loading"), false);
        assert!(
            refused[0].contains("503 Service Unavailable: loading"),
            "{refused:?}"
        );
        assert!(refused[1].contains("still running"));
    }
}
