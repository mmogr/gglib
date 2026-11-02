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
const REQUIRED: [&str; 23] = [
    "/props\tdefault_generation_settings.n_ctx\tnumber",
    "/props\tbuild_info\tstring",
    "/props\tchat_template\tstring",
    "/health\tstatus\tstring",
    "/apply-template\tprompt\tstring",
    "chat completion\tchoices[].message.content\tstring",
    "chat completion\tchoices[].message.reasoning_content\tstring",
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

/// The fields of a choice's `delta` or `message` the pinned llama-server
/// answers that nothing in gglib reads, as `endpoint<TAB>key path` without
/// the type, each with why. A field the recording gains that is in neither
/// this list nor [`REQUIRED`] fails
/// [`every_choice_field_llama_server_answers_is_read_or_known`]: upstream
/// has put something new in the object gglib reads, and it is read, or it is
/// named here with its reason.
const UNREAD_CHOICE_FIELDS: [(&str, &str); 1] = [(
    "chat completion\tchoices[].message.role",
    "always `assistant`; the proxy forwards the stream's `delta.role`, which ggchat reads",
)];

/// A `shape.txt` line's endpoint and key path, without its type: one field
/// is one name, whatever types it has taken (a stream's `delta.content` is a
/// string, then `null` on the last chunk).
fn field(line: &str) -> &str {
    line.rsplit_once('\t').map_or(line, |(field, _)| field)
}

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

/// What the pinned llama-server puts in a choice's `delta` or `message` is
/// either read (in [`REQUIRED`]) or known and left (in
/// [`UNREAD_CHOICE_FIELDS`]); and nothing is known that it no longer answers.
#[test]
fn every_choice_field_llama_server_answers_is_read_or_known() {
    let answered: BTreeSet<&str> = LLAMA_SHAPE
        .lines()
        .filter(|line| line.contains("\tchoices[].delta.") || line.contains("\tchoices[].message."))
        .map(field)
        .collect();
    let read: BTreeSet<&str> = REQUIRED.into_iter().map(field).collect();
    let known: BTreeSet<&str> = UNREAD_CHOICE_FIELDS.into_iter().map(|(l, _)| l).collect();
    let new: Vec<&str> = answered
        .iter()
        .copied()
        .filter(|line| !read.contains(line) && !known.contains(line))
        .collect();
    assert!(
        new.is_empty(),
        "llama-server now answers fields of a choice gglib neither reads nor knows: {new:#?}\n\
         Read each, or add it to UNREAD_CHOICE_FIELDS with why it is left."
    );
    let choice_read: BTreeSet<&str> = read
        .iter()
        .copied()
        .filter(|f| f.contains("\tchoices[].delta.") || f.contains("\tchoices[].message."))
        .collect();
    let gone: Vec<&str> = known
        .iter()
        .chain(&choice_read)
        .copied()
        .filter(|field| !answered.contains(field))
        .collect();
    assert!(
        gone.is_empty(),
        "REQUIRED or UNREAD_CHOICE_FIELDS names fields llama-server no longer answers: {gone:#?}"
    );
    let both: Vec<&str> = known.intersection(&choice_read).copied().collect();
    assert!(both.is_empty(), "read and listed as unread: {both:?}");
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
