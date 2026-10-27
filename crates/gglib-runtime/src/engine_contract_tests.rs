//! The pinned llama-server's own answers, recorded into `contracts/upstream/`.
//!
//! CI starts the pinned llama-server on two tiny models, through gglib's own
//! launch path, asks it what gglib asks, and keeps an allowlisted projection
//! of each answer: `shape.txt` (endpoint, key path, JSON type), `help.txt`,
//! `version.txt` and `observed.json`, plus sd-server's `help.txt` and
//! `version.txt`. Nothing the model generated and nothing rendered from a
//! prompt enters a projection (see `engine_contract_project`). It records
//! twice, from fresh processes, and fails if the two differ.
//!
//! Each `observed.json` leaf an ADR's criterion names carries the quote; the
//! hand-kept `approvals.json` beside it is what `scripts/check_adr_readings.sh`
//! holds it to. Redacted raw bodies go to `GGLIB_ENGINE_BODIES_DIR` (CI's
//! artifact), and are committed under `llama/bodies/` only when a pin moves.
//!
//! Inputs: `GGLIB_ENGINES_DIR` (where gglib installed the engines) and
//! `GGLIB_ENGINE_MODELS_DIR` (the models, and `templates/` fetched at the
//! pin). It skips when the second is unset. By default it compares with the
//! committed files; `GGLIB_RECORD_CONTRACTS=1` rewrites them.

use std::path::PathBuf;
use std::time::Instant;

#[path = "engine_contract_launch.rs"]
mod launch;
#[path = "engine_contract_project.rs"]
mod project;
#[path = "engine_contract_readings.rs"]
mod readings;
#[path = "engine_contract_tier3.rs"]
mod tier3;

use launch::Inputs;
use project::{
    Recording, assert_nothing_local, help_default, line_diff, redact_stream, write_under,
};
use readings::record;

/// The two passes kept the same projections.
fn assert_stable(first: &Recording, second: &Recording) {
    for (name, (value, _)) in &first.observed {
        let again = second.observed.get(name).map(|(v, _)| v);
        assert!(
            again == Some(value),
            "recording is not stable: {name} differs between two runs on one binary \
             ({value} then {again:?}). Add it to the redactions in engine_contract_project.rs."
        );
    }
    let (a, b) = (first.files(), second.files());
    for (rel, text) in &a {
        let diff = line_diff(text, &b[rel]);
        assert!(
            diff.is_empty(),
            "recording is not stable: {rel} differs between two runs:\n{diff}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts the pinned llama-server; CI sets GGLIB_ENGINE_MODELS_DIR"]
async fn the_pinned_llama_server_answers_as_contracts_upstream_records() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let Some(inputs) = Inputs::from_env(scratch.path()) else {
        eprintln!("GGLIB_ENGINE_MODELS_DIR is unset: nothing recorded");
        return;
    };
    let started = Instant::now();
    let first = record(&inputs).await;
    let second = record(&inputs).await;
    eprintln!("recorded twice in {:.1} s", started.elapsed().as_secs_f64());
    assert_stable(&first, &second);
    let files = first.files();
    for (name, text) in files
        .iter()
        .map(|(n, t)| (*n, t))
        .chain(first.bodies.iter().map(|(n, t)| (n.as_str(), t)))
    {
        assert_nothing_local(name, text);
    }
    let bodies = std::env::var_os("GGLIB_ENGINE_BODIES_DIR")
        .map_or_else(|| inputs.scratch_path("bodies"), PathBuf::from);
    for (name, body) in &first.bodies {
        write_under(&bodies, name, body);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/upstream");
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        for (rel, text) in &files {
            write_under(&root, rel, text);
        }
        return;
    }
    let stale: Vec<String> = files
        .iter()
        .filter_map(|(rel, want)| {
            let have = std::fs::read_to_string(root.join(rel)).unwrap_or_default();
            (have != *want).then(|| format!("{rel}:\n{}", line_diff(&have, want)))
        })
        .collect();
    assert!(
        stale.is_empty(),
        "contracts/upstream is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{}",
        stale.join("\n")
    );
}

/// The recorded `--version` is the pinned release's, so a pin moved without
/// recording again fails in every test run, not only on the engine legs.
#[test]
fn the_recording_is_of_the_pinned_llama_server() {
    let version = include_str!("../../../contracts/upstream/llama/version.txt");
    let build = gglib_core::domain::RuntimeCapabilities::from_version_output(version).build;
    assert_eq!(
        build.map(|b| format!("b{b}")).as_deref(),
        Some(crate::llama::PINNED_LLAMA_RELEASE),
        "contracts/upstream/llama/version.txt is {version:?}; record it again at the pin"
    );
}

/// A redacted body keeps every key and loses what was generated or is local.
#[test]
fn a_redacted_stream_keeps_its_keys_and_drops_what_was_generated() {
    let body = "data: {\"id\":\"chatcmpl-1\",\"created\":17,\"choices\":[{\"delta\":{\"content\":\"Once\"},\"finish_reason\":null}],\"timings\":{\"prompt_ms\":1.5}}\n\ndata: [DONE]\n";
    assert_eq!(
        redact_stream(body),
        "data: {\"choices\":[{\"delta\":{\"content\":\"<redacted>\"},\"finish_reason\":null}],\"created\":0,\"id\":\"<redacted>\",\"timings\":{}}\n\ndata: [DONE]\n"
    );
}

/// `--help` defaults are read from the option's own text, quotes and all.
#[test]
fn a_help_default_is_read_from_its_own_option() {
    let help = "-b,    --batch-size N    logical maximum batch size (default: 2048)\n\
                -fit,  --fit [on|off]    whether to fit ('on' or\n\
                \x20                         'off', default: 'on')\n\
                -np,   --parallel N      number of server slots (default: -1, -1 = auto)\n";
    assert_eq!(help_default(help, "--batch-size").as_deref(), Some("2048"));
    assert_eq!(help_default(help, "--fit").as_deref(), Some("on"));
    assert_eq!(help_default(help, "-np").as_deref(), Some("-1"));
    assert_eq!(help_default(help, "--jinja"), None);
}

/// A path on the recording machine never reaches a committed file.
#[test]
#[should_panic(expected = "would commit")]
fn a_recorded_path_on_this_machine_is_refused() {
    assert_nothing_local("sd/help.txt", "Usage: /Users/x/sd-server");
}

/// Whatever its case.
#[test]
#[should_panic(expected = "would commit")]
fn a_recorded_path_is_refused_in_any_case() {
    assert_nothing_local("sd/help.txt", "Usage: /HOME/x/sd-server");
}

#[test]
fn a_bare_binary_name_is_kept() {
    assert_nothing_local("sd/help.txt", "Usage: sd-server [options]");
}
