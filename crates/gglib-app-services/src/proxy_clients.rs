//! How a client connects to the endpoint, said once by the daemon.
//!
//! `gglib up` ended with a block of values a person retypes into Cline,
//! Continue or `Open WebUI`, and the Endpoint page's "Connect a client" shows
//! the same values. Both are the daemon's facts (where the endpoint is
//! reached, whether a request needs the key, the id a client sends for the
//! model) arranged per client, so the arrangement lives here, behind
//! `GET /api/proxy/clients?model=`, and every surface prints it rather than
//! composing it.
//!
//! The key itself never travels: a recipe that needs one carries a
//! placeholder and [`ClientRecipes::key`] says `required`; the surface that
//! has the reveal command (`gglib_core::secret`) says where the real one is.

use gglib_core::domain::KeyState;
use serde::{Deserialize, Serialize};

/// The placeholder a recipe writes where a client wants a key the endpoint
/// does not check: most clients refuse an empty field.
pub const KEY_PLACEHOLDER: &str = "gglib";

/// The facts a recipe is written from: the endpoint's address as the daemon
/// says it, the key rule, and the model as a client sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientFacts {
    /// `http://<address>/v1`, the daemon's own dial rule.
    pub base_url: String,
    /// Whether a request must carry the key; `None` when the daemon could
    /// not read its settings to say.
    pub key: Option<KeyState>,
    /// The id a client sends: the model's name, or `name@QUANT` when two
    /// share a name, as `/v1/models` lists it.
    pub model_id: String,
    /// gglib's own id for it, which a printed command takes as one argument.
    pub gglib_id: i64,
}

/// One copyable line of a recipe: a label and the value to paste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RecipeLine {
    pub label: String,
    pub value: String,
}

/// The lines for one client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ClientRecipe {
    /// The client, as its users call it: `opencode`, `Cline / Roo / Continue`.
    pub client: String,
    /// Where in the client the lines go, or what to know first.
    pub hint: String,
    /// The lines, in the order they are pasted.
    pub lines: Vec<RecipeLine>,
}

/// What `GET /api/proxy/clients?model=` answers: the three facts every
/// client wants, and the recipes written from them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ClientRecipes {
    /// `http://<address>/v1`.
    pub base_url: String,
    /// Whether a request must carry the key, in the word every surface
    /// uses; `None` when the daemon could not read its settings to say.
    pub key: Option<KeyState>,
    /// The id a client sends for the model.
    pub model_id: String,
    /// gglib's own id for it.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub gglib_id: i64,
    /// One recipe per client, in the order the page lists them.
    pub recipes: Vec<ClientRecipe>,
}

/// The recipes for `facts`, in the order every surface lists them.
#[must_use]
pub fn recipes(facts: &ClientFacts) -> ClientRecipes {
    let base = facts.base_url.as_str();
    let model = facts.model_id.as_str();
    // A key the endpoint checks is never written here; its line names the
    // fact and the surface says where the key is. One the endpoint does not
    // check is a placeholder, since most clients refuse an empty field.
    let key_value = match facts.key {
        Some(KeyState::Required) => "the stored key (required)".to_owned(),
        Some(KeyState::NoneNeeded) => {
            format!("{KEY_PLACEHOLDER}  (unused, but most clients demand one)")
        }
        None => "unknown (the daemon could not read its settings to say)".to_owned(),
    };
    let key_literal = match facts.key {
        Some(KeyState::Required) => "<your key>",
        _ => KEY_PLACEHOLDER,
    };
    let line = |label: &str, value: String| RecipeLine {
        label: label.to_owned(),
        value,
    };
    ClientRecipes {
        base_url: facts.base_url.clone(),
        key: facts.key,
        model_id: facts.model_id.clone(),
        gglib_id: facts.gglib_id,
        recipes: vec![
            ClientRecipe {
                client: "opencode".to_owned(),
                hint: "opencode.json, in the project or in ~/.config/opencode/".to_owned(),
                lines: vec![
                    line("provider.gglib.npm", "@ai-sdk/openai-compatible".to_owned()),
                    line("provider.gglib.options.baseURL", base.to_owned()),
                    line("provider.gglib.models", format!("{{ \"{model}\": {{}} }}")),
                    line("model", format!("gglib/{model}")),
                ],
            },
            ClientRecipe {
                client: "Cline / Roo / Continue".to_owned(),
                hint: "provider: OpenAI Compatible".to_owned(),
                lines: vec![
                    line("Base URL", base.to_owned()),
                    line("API Key", key_value),
                    line("Model ID", model.to_owned()),
                ],
            },
            ClientRecipe {
                client: "Open WebUI".to_owned(),
                hint: "Settings -> Connections -> OpenAI API".to_owned(),
                lines: vec![
                    line("URL", base.to_owned()),
                    line("Key", key_literal.to_owned()),
                ],
            },
            ClientRecipe {
                client: "VS Code Copilot (BYOK)".to_owned(),
                hint: "needs one fixed model, so serve only this one".to_owned(),
                lines: vec![line("Run", format!("gglib serve {}", facts.gglib_id))],
            },
            ClientRecipe {
                client: "Any OpenAI client".to_owned(),
                hint: "environment variables, as aider and most SDKs read them".to_owned(),
                lines: vec![
                    line("OPENAI_API_BASE", base.to_owned()),
                    line("OPENAI_API_KEY", key_literal.to_owned()),
                    line("model", model.to_owned()),
                ],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(key: Option<KeyState>) -> ClientFacts {
        ClientFacts {
            base_url: "http://127.0.0.1:8080/v1".to_owned(),
            key,
            model_id: "qwen3-30b-a3b@Q4_K_M".to_owned(),
            gglib_id: 7,
        }
    }

    /// The three values a person retypes reach every recipe that wants them.
    #[test]
    fn every_recipe_carries_the_address_and_the_model_as_a_client_sends_it() {
        let r = recipes(&facts(Some(KeyState::NoneNeeded)));
        assert_eq!(r.base_url, "http://127.0.0.1:8080/v1");
        assert_eq!(r.model_id, "qwen3-30b-a3b@Q4_K_M");
        assert_eq!(r.gglib_id, 7);
        let names: Vec<&str> = r.recipes.iter().map(|x| x.client.as_str()).collect();
        assert_eq!(
            names,
            [
                "opencode",
                "Cline / Roo / Continue",
                "Open WebUI",
                "VS Code Copilot (BYOK)",
                "Any OpenAI client"
            ]
        );
        // Copilot's recipe is the pinned command; Open WebUI reads the
        // models from the endpoint and wants no model line.
        for recipe in r
            .recipes
            .iter()
            .filter(|x| x.client != "VS Code Copilot (BYOK)")
        {
            let text: Vec<&str> = recipe.lines.iter().map(|l| l.value.as_str()).collect();
            assert!(
                text.iter().any(|v| v.contains("http://127.0.0.1:8080/v1")),
                "{}: {text:?}",
                recipe.client
            );
            if recipe.client != "Open WebUI" {
                assert!(
                    text.iter().any(|v| v.contains("qwen3-30b-a3b@Q4_K_M")),
                    "{}: {text:?}",
                    recipe.client
                );
            }
        }
    }

    /// Copilot cannot use the unpinned endpoint; its recipe is the pinned
    /// command with the model by id, which a shell passes as one argument.
    #[test]
    fn copilot_is_pointed_at_the_pinned_command_by_id() {
        let r = recipes(&facts(Some(KeyState::NoneNeeded)));
        let copilot = r
            .recipes
            .iter()
            .find(|x| x.client.starts_with("VS Code Copilot"))
            .unwrap();
        assert_eq!(copilot.lines[0].value, "gglib serve 7");
        assert!(!copilot.lines[0].value.contains("qwen"));
    }

    /// A key the endpoint checks is never written into a recipe: the line
    /// says one is required and the answer says so in its word.
    #[test]
    fn a_required_key_is_named_and_never_written() {
        let r = recipes(&facts(Some(KeyState::Required)));
        assert_eq!(r.key, Some(KeyState::Required));
        let all: Vec<String> = r
            .recipes
            .iter()
            .flat_map(|x| x.lines.iter().map(|l| l.value.clone()))
            .collect();
        assert!(all.iter().any(|v| v.contains("required")), "{all:?}");
        assert!(
            all.iter().all(|v| !v.contains(KEY_PLACEHOLDER)
                || v.starts_with("gglib serve")
                || v.starts_with("gglib/")),
            "{all:?}"
        );
    }

    /// With no key to check, clients that demand one get a placeholder.
    #[test]
    fn no_key_needed_gives_the_placeholder_most_clients_demand() {
        let r = recipes(&facts(Some(KeyState::NoneNeeded)));
        let webui = r.recipes.iter().find(|x| x.client == "Open WebUI").unwrap();
        assert_eq!(webui.lines[1].value, KEY_PLACEHOLDER);
        let unknown = recipes(&facts(None));
        assert_eq!(unknown.key, None);
        let cline = unknown
            .recipes
            .iter()
            .find(|x| x.client.starts_with("Cline"))
            .unwrap();
        assert!(cline.lines[1].value.starts_with("unknown"));
    }

    /// The answer's keys are the contract's, which the CLI's mirror is held to.
    #[test]
    fn the_answer_carries_exactly_the_fields_the_contract_names() {
        let value = serde_json::to_value(recipes(&facts(Some(KeyState::NoneNeeded)))).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut named = gglib_core::contracts::http::daemon::PROXY_CLIENTS_FIELDS.to_vec();
        named.sort_unstable();
        assert_eq!(keys, named);
    }
}
