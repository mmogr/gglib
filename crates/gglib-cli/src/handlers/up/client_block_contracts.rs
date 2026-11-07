//! The client block's two renderings, recorded for the Endpoint page.
//!
//! The page's Test it says the proof's sentence, and its Connect a client
//! lists the recipes' lines; both must say what `gglib proxy test` and
//! `gglib up` print. These tests record a few inputs beside what
//! [`render_proof`] and [`render_client_config`] make of each, in
//! `contracts/proxy/proof.json` and `contracts/proxy/client_config.json`,
//! and `tests/ts/contracts/connectClient.test.ts` holds the page's sentence
//! and lines to them. Run with `GGLIB_RECORD_CONTRACTS=1` to rewrite them
//! after a deliberate change.

use std::path::PathBuf;

use gglib_app_services::{ClientFacts, recipes};
use gglib_core::domain::KeyState;
use serde_json::{Value, json};

use super::{render_client_config, render_proof};
use crate::daemon_client::{ClientRecipesDto, ProofOutcomeDto};

/// Write `want` to `contracts/proxy/<name>` under `GGLIB_RECORD_CONTRACTS`,
/// then require the checked-in file to be exactly it.
fn write_or_compare(name: &str, recorded: &Value) {
    let mut want = serde_json::to_string_pretty(recorded).expect("serialise");
    want.push('\n');
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/proxy")
        .join(name);
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("make contracts/proxy");
        std::fs::write(&path, &want).expect("write the contract");
    }
    let have = std::fs::read_to_string(&path).expect("read the contract");
    assert!(
        have == want,
        "contracts/proxy/{name} is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}

/// The proofs recorded: the model asked for answered, another name for it
/// answered, a time that rounds up and one that rounds down, and a refusal.
#[test]
fn the_checked_in_proof_lines_are_what_the_cli_prints() {
    let answered = |answered_by: &str, asked_for: &str, elapsed_ms: u64| ProofOutcomeDto {
        answered_by: answered_by.to_owned(),
        asked_for: asked_for.to_owned(),
        elapsed_ms,
    };
    let mut recorded = Vec::new();
    for proof in [
        answered("Qwen3-4B", "Qwen3-4B", 1_350),
        answered("qwen3-8b", "Qwen3 8B", 949),
        answered("Gemma-3-4B-It", "Gemma-3-4B-It", 12_040),
        answered("Gemma-3-4B-It", "Gemma-3-4B-It", 40),
    ] {
        let line = render_proof(Ok(&proof), false);
        assert_eq!(line.len(), 1, "{line:?}");
        recorded.push(json!({
            "answered": {
                "answered_by": proof.answered_by,
                "asked_for": proof.asked_for,
                "elapsed_ms": proof.elapsed_ms,
            },
            "failed": null,
            "line": line[0],
        }));
    }
    let reason = "the endpoint did not answer /health within 30s";
    let line = render_proof(Err(reason), false);
    assert_eq!(line.len(), 1, "{line:?}");
    recorded.push(json!({ "answered": null, "failed": reason, "line": line[0] }));
    write_or_compare("proof.json", &Value::Array(recorded));
}

/// The daemon's recipes for one model with a key required and with none
/// needed, beside the block the CLI prints of each: the key lines are the
/// rule worth recording, since a required key is said as the stored secret.
#[test]
fn the_checked_in_client_configs_are_what_the_cli_prints() {
    let mut recorded = Vec::new();
    for key in [Some(KeyState::NoneNeeded), Some(KeyState::Required)] {
        let answer = serde_json::to_value(recipes(&ClientFacts {
            base_url: "http://127.0.0.1:8080/v1".to_owned(),
            key,
            model_id: "Qwen3-4B".to_owned(),
            gglib_id: 7,
        }))
        .expect("the daemon's answer serialises");
        let read: ClientRecipesDto =
            serde_json::from_value(answer.clone()).expect("the CLI reads the daemon's answer");
        recorded.push(json!({
            "recipes": answer,
            "lines": render_client_config(&read, false),
        }));
    }
    write_or_compare("client_config.json", &Value::Array(recorded));
}
