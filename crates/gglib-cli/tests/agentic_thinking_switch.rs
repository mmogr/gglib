//! `gglib config settings set --agentic-thinking`, run as a person runs it
//! against a database on disk, and what `gglib q` does with what it stores.
//!
//! The policy is the proxy's. The built-in agent behind `gglib q` follows its
//! own `--thinking`, so a stored policy that would set a budget on the proxy
//! sets none on its turn. What the policy does on the proxy is
//! `gglib-proxy`'s `integration_agentic_thinking`.

use std::path::Path;
use std::process::{Command, Stdio};

use gglib_core::AgenticThinking;
use gglib_core::domain::{InferenceConfig, ModelCapabilities, NewModel};
use serde_json::{Value, json};

#[path = "support/data_dir.rs"]
mod data_dir;
#[path = "support/model_server.rs"]
mod model_server;

/// Run `gglib config settings <args>` against `root` and return its stdout.
fn settings(root: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["config", "settings"])
        .args(args)
        .env("GGLIB_DATA_DIR", root)
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib config settings`");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "`gglib config settings {args:?}` must succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    stdout
}

/// The `agentic-thinking` row of `gglib config settings show`.
fn shown(root: &Path) -> String {
    let stdout = settings(root, &["show"]);
    let row = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("agentic-thinking"))
        .unwrap_or_else(|| panic!("no agentic-thinking row in:\n{stdout}"));
    row.trim_start()
        .trim_start_matches("agentic-thinking")
        .trim()
        .to_owned()
}

/// Each value the flag takes is stored as its own policy and shown under the
/// word that was typed; unsetting it returns to the unset default.
#[test]
fn the_flag_stores_each_policy_and_unset_clears_it() {
    let root = tempfile::tempdir().expect("temp data dir");
    assert_eq!(data_dir::read_settings(root.path()).agentic_thinking, None);
    assert_eq!(shown(root.path()), "None (when-asked)");

    for (typed, stored) in [
        ("off", AgenticThinking::Off),
        ("brief", AgenticThinking::Brief),
        ("default", AgenticThinking::Default),
        ("when-asked", AgenticThinking::WhenAsked),
    ] {
        let said = settings(root.path(), &["set", "--agentic-thinking", typed]);
        assert!(said.contains("agentic-thinking"), "{said}");
        assert_eq!(
            data_dir::read_settings(root.path()).agentic_thinking,
            Some(stored),
            "--agentic-thinking {typed}"
        );
        assert_eq!(shown(root.path()), typed);
    }

    settings(root.path(), &["unset", "agentic-thinking"]);
    assert_eq!(data_dir::read_settings(root.path()).agentic_thinking, None);
    assert_eq!(shown(root.path()), "None (when-asked)");
}

#[test]
fn a_value_the_flag_does_not_know_is_refused_and_stores_nothing() {
    let root = tempfile::tempdir().expect("temp data dir");
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args([
            "config",
            "settings",
            "set",
            "--agentic-thinking",
            "sometimes",
        ])
        .env("GGLIB_DATA_DIR", root.path())
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib config settings set`");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("when-asked"),
        "the refusal lists the values: {stderr}"
    );
    assert_eq!(data_dir::read_settings(root.path()).agentic_thinking, None);
}

/// `off` would send a budget of 0 on any proxy turn with tools. `gglib q` on
/// the same library sends a turn with tools and no budget. The global `top_k`
/// stored beside the policy is in the request, so these settings were the
/// ones read: a model the library does not hold is sent neither.
#[test]
fn a_stored_policy_sets_no_budget_on_the_built_in_agents_turn() {
    let root = tempfile::tempdir().expect("temp data dir");
    let path = std::path::PathBuf::from("/models/served.gguf");
    let mut model = NewModel::new("served".to_owned(), path, 7.0, chrono::Utc::now());
    model.capabilities =
        ModelCapabilities::SUPPORTS_TOOL_CALLS | ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
    data_dir::add_model(root.path(), model);
    data_dir::write_settings(root.path(), |settings| {
        settings.agentic_thinking = Some(AgenticThinking::Off);
        settings.inference_defaults = Some(InferenceConfig {
            top_k: Some(33),
            ..InferenceConfig::default()
        });
    });

    let (port, completions) = model_server::model_server();
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["q", "hi", "-m", "served", "--port", &port.to_string()])
        .env("GGLIB_DATA_DIR", root.path())
        .env_remove("GGLIB_DISABLE_AGENTIC_SAMPLING")
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib q`");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success() && stdout.contains("ok"),
        "`gglib q` must answer\nstdout: {stdout}\nstderr: {stderr}"
    );

    let body = completions.try_recv().expect("a completion request");
    let body: Value = serde_json::from_str(&body).expect("a JSON body");
    let tools = body["tools"].as_array();
    assert!(tools.is_some_and(|tools| !tools.is_empty()), "{body}");
    assert_eq!(body["top_k"], json!(33), "{body}");
    assert!(body.get("reasoning_budget_tokens").is_none(), "{body}");
}
