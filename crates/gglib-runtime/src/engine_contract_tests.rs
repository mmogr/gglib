//! The pinned llama-server's own answers, recorded into `contracts/upstream/`,
//! and the in-process engine's, held to them.
//!
//! CI starts the pinned llama-server on two tiny models, through gglib's own
//! launch path, asks it what gglib asks, and keeps an allowlisted projection
//! of each answer: `shape.txt` (endpoint, key path, JSON type), `help.txt`,
//! `version.txt` and `observed.json`, plus sd-server's `help.txt` and
//! `version.txt`. Nothing the model generated and nothing rendered from a
//! prompt enters a projection but the readings `engine_contract_project`
//! names. It records twice, from fresh processes, and fails if the two
//! differ.
//!
//! Each `observed.json` leaf an ADR's criterion names carries the quote; the
//! hand-kept `approvals.json` beside it is what `scripts/check_adr_readings.sh`
//! holds it to. Redacted raw bodies, and the prompts the dateless templates
//! render for one fixed chat, verbatim, go to `GGLIB_ENGINE_BODIES_DIR` (CI's
//! artifact), and are committed under `llama/bodies/` only when a pin moves.
//!
//! Inputs: `GGLIB_ENGINES_DIR` (where gglib installed the engines) and
//! `GGLIB_ENGINE_MODELS_DIR` (the models, and `templates/` fetched at the
//! pin). It skips when the second is unset. By default it compares with the
//! committed files; `GGLIB_RECORD_CONTRACTS=1` rewrites them.
//!
//! The same recorder asks gglib-engine-llama in this process the same
//! questions on the same models (`engine_contract_inproc.rs`) into
//! `llama-inproc/`, with its stream and its renders, when
//! `GGLIB_INPROC_ENGINE=1` (`make record-upstream-inproc`). Tests on every OS
//! hold what is committed there to llama-server's recording
//! (`engine_contract_parity.rs`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

#[path = "engine_contract_gemma.rs"]
mod gemma;
#[path = "engine_contract_inproc.rs"]
mod inproc;
#[path = "engine_contract_launch.rs"]
mod launch;
#[path = "engine_contract_parity.rs"]
mod parity;
#[path = "engine_contract_project.rs"]
mod project;
#[path = "engine_contract_readings.rs"]
mod readings;
#[path = "engine_contract_tier3.rs"]
mod tier3;

use inproc::Upstream;
use launch::Inputs;
use project::{
    Recording, assert_nothing_local, help_default, line_diff, redact_stream, upstream_dir,
    write_under,
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
    for (name, prompt) in &first.prompts {
        assert!(
            second.prompts.get(name) == Some(prompt),
            "recording is not stable: {name} rendered differently between two runs"
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

/// Two passes of `upstream`, which must agree and name nothing on this
/// machine: the first.
async fn record_twice(upstream: Upstream<'_>) -> Recording {
    let started = Instant::now();
    let first = record(upstream).await;
    let second = record(upstream).await;
    eprintln!("recorded twice in {:.1} s", started.elapsed().as_secs_f64());
    assert_stable(&first, &second);
    for (name, text) in first
        .files()
        .iter()
        .chain(&first.bodies)
        .chain(&first.prompts)
    {
        assert_nothing_local(name, text);
    }
    first
}

/// Write `files` into `contracts/upstream/` under `GGLIB_RECORD_CONTRACTS`,
/// or fail naming each one that differs from what is committed.
fn write_or_compare(files: &BTreeMap<String, String>, again: &str) {
    let root = upstream_dir();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        for (rel, text) in files {
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
        "contracts/upstream is stale; {again}\n{}",
        stale.join("\n")
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts the pinned llama-server; CI sets GGLIB_ENGINE_MODELS_DIR"]
async fn the_pinned_llama_server_answers_as_contracts_upstream_records() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let Some(inputs) = Inputs::from_env(scratch.path()) else {
        eprintln!("GGLIB_ENGINE_MODELS_DIR is unset: nothing recorded");
        return;
    };
    let first = record_twice(Upstream::LlamaServer(&inputs)).await;
    let bodies = std::env::var_os("GGLIB_ENGINE_BODIES_DIR")
        .map_or_else(|| inputs.scratch_path("bodies"), PathBuf::from);
    for (name, body) in first.bodies.iter().chain(&first.prompts) {
        write_under(&bodies, name, body);
    }
    write_or_compare(&first.files(), "rerun with GGLIB_RECORD_CONTRACTS=1");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs llama.cpp in this process; make record-upstream-inproc sets GGLIB_INPROC_ENGINE=1"]
async fn the_in_process_engine_answers_as_contracts_upstream_records() {
    let Some(inputs) = inproc::Inputs::from_env() else {
        eprintln!("GGLIB_INPROC_ENGINE is not 1: the in-process engine is not recorded");
        return;
    };
    let first = record_twice(Upstream::InProcess(&inputs)).await;
    write_or_compare(&first.files(), "run make record-upstream-inproc on a Mac");
}

/// The recorded `--version` is the pinned release's, so a pin moved without
/// recording again fails in every test run, not only on the engine legs.
#[test]
fn the_recording_is_of_the_pinned_llama_server() {
    let version = include_str!("../../../contracts/upstream/llama/version.txt");
    let build = gglib_core::domain::RuntimeCapabilities::from_version_output(version).build;
    assert_eq!(
        build.map(|b| format!("b{b}")).as_deref(),
        Some(gglib_core::domain::engine_pins::PINNED_LLAMA_RELEASE),
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

/// Both passes must render the fixed chat alike, byte for byte.
#[test]
#[should_panic(expected = "rendered differently")]
fn a_prompt_the_second_pass_renders_differently_is_refused() {
    let pass = |prompt: &str| {
        let mut r = Recording::default();
        r.prompts.insert(
            "apply-template.t.thinking.txt".to_owned(),
            prompt.to_owned(),
        );
        r
    };
    assert_stable(&pass("<think>\n"), &pass("<think>\n\n"));
}
