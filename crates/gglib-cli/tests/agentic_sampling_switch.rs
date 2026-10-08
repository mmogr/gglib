//! `GGLIB_DISABLE_AGENTIC_SAMPLING` in the environment of `gglib q`, run as a
//! person runs it against a model server on `--port`.
//!
//! An ordinary model with nothing chosen resolves the floor's 0.7 and is sent
//! 0.3 on a turn with tools while the agentic ceiling applies. The variable
//! switches the ceiling off whatever the settings store, for a model in the
//! library and for a server whose model the library does not hold. Only a
//! process of its own can hold the variable, so this runs the built binary;
//! what the stored setting does is `config_sampling_tests`'.

use std::path::Path;
use std::process::{Command, Stdio};

use gglib_core::domain::{ModelCapabilities, NewModel};
use serde_json::{Value, json};

#[path = "support/data_dir.rs"]
mod data_dir;
#[path = "support/model_server.rs"]
mod model_server;

const SWITCH: &str = "GGLIB_DISABLE_AGENTIC_SAMPLING";

/// A data directory whose catalogue holds `served`, a model that can call
/// tools, and whose settings store `agentic_sampling` as `stored`.
fn library(stored: Option<bool>) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp data dir");
    let path = std::path::PathBuf::from("/models/served.gguf");
    let mut model = NewModel::new("served".to_owned(), path, 7.0, chrono::Utc::now());
    model.capabilities =
        ModelCapabilities::SUPPORTS_TOOL_CALLS | ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
    data_dir::add_model(root.path(), model);
    data_dir::write_settings(root.path(), |settings| settings.agentic_sampling = stored);
    root
}

/// The temperature `gglib q hi -m <model> --port <a model server>` sent on
/// its turn with tools, with [`SWITCH`] set to `value` when one is given and
/// otherwise absent.
fn sent(root: &Path, model: &str, value: Option<&str>) -> Value {
    let (port, completions) = model_server::model_server();
    let mut command = Command::new(env!("CARGO_BIN_EXE_gglib"));
    command
        .args(["q", "hi", "-m", model, "--port", &port.to_string()])
        .env("GGLIB_DATA_DIR", root)
        .env_remove(SWITCH)
        .stdin(Stdio::null());
    if let Some(value) = value {
        command.env(SWITCH, value);
    }
    let out = command.output().expect("running `gglib q`");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success() && stdout.contains("ok"),
        "`gglib q -m {model}` must answer\nstdout: {stdout}\nstderr: {stderr}"
    );
    let body = completions.try_recv().expect("a completion request");
    let body: Value = serde_json::from_str(&body).expect("a JSON body");
    let tools = body["tools"].as_array();
    assert!(tools.is_some_and(|tools| !tools.is_empty()), "{body}");
    body["temperature"].clone()
}

/// With the setting stored on, or not stored, the turn is capped; the
/// variable set truthy sends the resolved 0.7 instead, and set to a value
/// that is not truthy changes nothing.
#[test]
fn the_environment_switch_turns_the_ceiling_off_whatever_is_stored() {
    for stored in [None, Some(true)] {
        let root = library(stored);
        for model in ["served", "stranger"] {
            let sent_with = |value| sent(root.path(), model, value);
            assert_eq!(sent_with(None), json!(0.3_f32), "{model}, {stored:?}");
            assert_eq!(sent_with(Some("0")), json!(0.3_f32), "{model}, {stored:?}");
            assert_eq!(sent_with(Some("1")), json!(0.7_f32), "{model}, {stored:?}");
        }
    }
}
