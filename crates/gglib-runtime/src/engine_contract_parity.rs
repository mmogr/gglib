//! The in-process engine held to the pinned llama-server, from the committed
//! recordings alone, on every OS.
//!
//! `contracts/upstream/llama/` is the oracle: what the pinned llama-server
//! answered. `contracts/upstream/llama-inproc/` is what gglib-engine-llama's
//! shim answered the same questions with, on the same models, over llama.cpp
//! in-process at the same pin. Its shape must be a part of the oracle's,
//! endpoint by endpoint, and carry the lines gglib reads; its renders of
//! each pinned template must be the oracle's byte for byte; and tinygemma3's
//! greedy reply must be the same visible pieces from the same prompt, as
//! many tokens, ending for the same reason.

use std::collections::BTreeSet;

use gglib_core::domain::engine_pins::PINNED_LLAMA_RELEASE;
use serde_json::Value;

use super::gemma::GREEDY;
use super::project::{line_diff, upstream_dir};
use super::readings::{RENDERS, TEMPLATES};

const LLAMA_SHAPE: &str = include_str!("../../../contracts/upstream/llama/shape.txt");
const INPROC_SHAPE: &str = include_str!("../../../contracts/upstream/llama-inproc/shape.txt");
const LLAMA_OBSERVED: &str = include_str!("../../../contracts/upstream/llama/observed.json");
const INPROC_OBSERVED: &str =
    include_str!("../../../contracts/upstream/llama-inproc/observed.json");
const INPROC_VERSION: &str = include_str!("../../../contracts/upstream/llama-inproc/version.txt");

/// The lines without which the in-process recording says nothing, one or
/// more for each endpoint the recorder asks: what gglib-proxy forwards and
/// reads, and what ggchat reads of a stream.
const REQUIRED: [&str; 22] = [
    "/props\tdefault_generation_settings.n_ctx\tnumber",
    "/props\tbuild_info\tstring",
    "/props\tchat_template\tstring",
    "/health\tstatus\tstring",
    "/apply-template\tprompt\tstring",
    "chat completion\tchoices[].message.content\tstring",
    "chat completion\tchoices[].finish_reason\tstring",
    "chat completion\tusage.prompt_tokens\tnumber",
    "chat completion\tusage.prompt_tokens_details.cached_tokens\tnumber",
    "stream chunk\tchoices[].delta.role\tstring",
    "stream chunk\tchoices[].delta.content\tstring",
    "stream chunk\tchoices[].delta.reasoning_content\tstring",
    "stream chunk\tchoices[].finish_reason\tstring",
    "stream chunk\tprompt_progress.total\tnumber",
    "stream chunk\tprompt_progress.processed\tnumber",
    "stream chunk\tprompt_progress.cache\tnumber",
    "stream chunk\tusage.prompt_tokens\tnumber",
    "stream chunk\tusage.completion_tokens\tnumber",
    "stream chunk\tusage.total_tokens\tnumber",
    "stream chunk\tusage.prompt_tokens_details.cached_tokens\tnumber",
    "stream chunk\ttimings.predicted_n\tnumber",
    "error (over context)\terror.n_ctx\tnumber",
];

/// Every line of `inproc` is one `llama` has: fails naming each that is not,
/// with its endpoint, and whether llama-server was asked that endpoint at all.
fn assert_subset(inproc: &str, llama: &str) {
    let answered: BTreeSet<&str> = llama.lines().collect();
    let endpoints: BTreeSet<&str> = llama.lines().filter_map(|l| l.split('\t').next()).collect();
    let extra: Vec<String> = inproc
        .lines()
        .filter(|line| !answered.contains(line))
        .map(|line| {
            let endpoint = line.split('\t').next().unwrap_or(line);
            let asked = if endpoints.contains(endpoint) {
                ""
            } else {
                " (llama-server was never asked this endpoint)"
            };
            format!("  {}{asked}", line.replace('\t', "  "))
        })
        .collect();
    assert!(
        extra.is_empty(),
        "llama-inproc/shape.txt has lines llama/shape.txt does not: the in-process engine \
         answers what the pinned llama-server never did\n{}",
        extra.join("\n")
    );
}

/// The committed file `rel` under `contracts/upstream/`.
fn committed(rel: &str) -> String {
    let path = upstream_dir().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The value of the reading `name` in an `observed.json`, null when absent.
fn reading(observed: &str, name: &str) -> Value {
    let observed: Value = serde_json::from_str(observed).expect("observed.json is JSON");
    observed[name]["value"].clone()
}

#[test]
fn the_in_process_engine_answers_in_the_pinned_llama_server_s_shape() {
    assert_subset(INPROC_SHAPE, LLAMA_SHAPE);
}

#[test]
fn the_in_process_recording_carries_the_lines_gglib_reads() {
    let lines: BTreeSet<&str> = INPROC_SHAPE.lines().collect();
    let missing: Vec<&str> = REQUIRED
        .into_iter()
        .filter(|line| !lines.contains(line))
        .collect();
    assert!(
        missing.is_empty(),
        "llama-inproc/shape.txt lacks lines gglib reads: {missing:#?}"
    );
}

/// `build_info` is gglib's pin constant as the shim's `/props` serves it,
/// and `llama.xcframework` the `pin.txt` beside the framework the recording
/// ran on: a pin moved without recording again fails every test run.
#[test]
fn the_in_process_recording_is_of_llama_cpp_at_the_pin() {
    for key in ["build_info", "llama.xcframework"] {
        let named = INPROC_VERSION
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(": "));
        assert_eq!(
            named,
            Some(PINNED_LLAMA_RELEASE),
            "contracts/upstream/llama-inproc/version.txt is {INPROC_VERSION:?}; \
             run make record-upstream-inproc at the pin"
        );
    }
}

#[test]
fn each_pinned_template_renders_in_process_as_llama_server_renders_it() {
    for template in TEMPLATES {
        for (variant, _) in RENDERS {
            let name = format!("bodies/apply-template.{template}.{variant}.txt");
            let (ours, theirs) = (
                committed(&format!("llama-inproc/{name}")),
                committed(&format!("llama/{name}")),
            );
            assert!(
                ours == theirs,
                "{name}: the in-process render is not llama-server's:\n{}",
                line_diff(&theirs, &ours)
            );
        }
    }
}

#[test]
fn tinygemma3_s_greedy_reply_in_process_is_llama_server_s() {
    for field in GREEDY {
        let name = format!("greedy.tinygemma3.{field}");
        let theirs = reading(LLAMA_OBSERVED, &name);
        assert!(
            theirs.as_array().is_none_or(|pieces| !pieces.is_empty()) && !theirs.is_null(),
            "llama/observed.json reads {name} as {theirs}: run make record-upstream"
        );
        let ours = reading(INPROC_OBSERVED, &name);
        assert_eq!(
            ours, theirs,
            "{name}: the in-process engine's (left) is not the pinned llama-server's (right)"
        );
    }
}

/// A line llama-server never answered is named, with its endpoint.
#[test]
#[should_panic(expected = "stream chunk  choices[].delta.tool_calls  array")]
fn a_line_llama_server_never_answered_is_named() {
    assert_subset(
        "stream chunk\tchoices[].delta.content\tstring\nstream chunk\tchoices[].delta.tool_calls\tarray\n",
        "stream chunk\tchoices[].delta.content\tstring\n",
    );
}

/// So is an endpoint it was never asked.
#[test]
#[should_panic(expected = "/metrics  up  bool (llama-server was never asked this endpoint)")]
fn an_endpoint_llama_server_was_never_asked_is_named() {
    assert_subset("/metrics\tup\tbool\n", "/health\tstatus\tstring\n");
}
