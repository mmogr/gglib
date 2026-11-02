//! The objects gglib reads from llama-server, held to what the pinned build
//! answers (`contracts/upstream/llama/shape.txt`).
//!
//! A key the pinned build adds to an object gglib parses is either read, or
//! named here as known and left, with why. Neither happens by itself: an
//! `Option` field reads what is there and a `serde` struct ignores what is
//! not named, so a new key would read forever as absent. These tests are
//! where it stops. `template_caps_tests` does the same for
//! `chat_template_caps`, and `engine_contract_parity` for a choice's
//! `delta` and `message`.

use std::collections::BTreeSet;

use crate::sampling_audit::SlotParams;

const SHAPE: &str = include_str!("../../../contracts/upstream/llama/shape.txt");

/// The `/slots` `params` the pinned llama-server reports that
/// [`SlotParams`] does not read. The table is read selectively on purpose
/// (`sampling_audit`'s module doc says why only the fields gglib resolves
/// are named); this list is what makes the choice explicit, so that a
/// parameter the build adds is seen once, here, and either read or added.
const UNREAD_SLOT_PARAMS: [&str; 40] = [
    "adaptive_decay",
    "adaptive_target",
    "backend_sampling",
    "chat_format",
    "dry_allowed_length",
    "dry_base",
    "dry_penalty_last_n",
    "dry_sequence_breakers",
    "dynatemp_exponent",
    "dynatemp_range",
    "frequency_penalty",
    "generation_prompt",
    "grammar",
    "grammar_lazy",
    "grammar_triggers",
    "ignore_eos",
    "logit_bias",
    "lora",
    "max_tokens",
    "min_keep",
    "mirostat",
    "mirostat_eta",
    "mirostat_tau",
    "n_discard",
    "n_keep",
    "n_predict",
    "n_probs",
    "post_sampling_probs",
    "preserved_tokens",
    "reasoning_format",
    "reasoning_in_content",
    "repeat_last_n",
    "speculative",
    "stop",
    "stream",
    "timings_per_token",
    "top_n_sigma",
    "typical_p",
    "xtc_probability",
    "xtc_threshold",
];

/// The top-level `params` keys `shape.txt` records under `/slots (debug)`.
fn recorded_slot_params() -> BTreeSet<&'static str> {
    SHAPE
        .lines()
        .filter_map(|line| line.strip_prefix("/slots (debug)\t[].params."))
        .map(|rest| rest.split(['\t', '.', '[']).next().unwrap_or(rest))
        .collect()
}

/// The keys a fully-populated [`SlotParams`] serialises.
fn read_slot_params() -> BTreeSet<String> {
    let full = SlotParams {
        temperature: Some(0.0),
        top_p: Some(0.0),
        top_k: Some(0.0),
        repeat_penalty: Some(0.0),
        presence_penalty: Some(0.0),
        min_p: Some(0.0),
        dry_multiplier: Some(0.0),
        seed: Some(0.0),
        samplers: Some(Vec::new()),
    };
    let value = serde_json::to_value(&full).expect("SlotParams serialises");
    value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect()
}

/// Every `/slots` parameter the pinned build reports is read by
/// [`SlotParams`] or known in [`UNREAD_SLOT_PARAMS`], and nothing is known
/// or read that the build no longer reports.
#[test]
fn every_slot_param_llama_server_reports_is_read_or_known() {
    let recorded = recorded_slot_params();
    assert!(
        recorded.len() > 30,
        "shape.txt records {} /slots params; the recorder asked /slots with debug on?",
        recorded.len()
    );
    let read = read_slot_params();
    let known: BTreeSet<&str> = UNREAD_SLOT_PARAMS.into_iter().collect();
    let new: Vec<&str> = recorded
        .iter()
        .copied()
        .filter(|key| !read.contains(*key) && !known.contains(key))
        .collect();
    assert!(
        new.is_empty(),
        "llama-server now reports /slots params gglib neither reads nor knows: {new:#?}\n\
         Read each in SlotParams, or add it to UNREAD_SLOT_PARAMS."
    );
    let gone: Vec<&str> = known
        .iter()
        .copied()
        .filter(|key| !recorded.contains(key))
        .chain(
            read.iter()
                .map(String::as_str)
                .filter(|key| !recorded.contains(key)),
        )
        .collect();
    assert!(
        gone.is_empty(),
        "names /slots params llama-server no longer reports: {gone:#?}"
    );
}

/// A known parameter is never also a read one.
#[test]
fn an_unread_slot_param_is_not_also_read() {
    let read = read_slot_params();
    let both: Vec<&str> = UNREAD_SLOT_PARAMS
        .into_iter()
        .filter(|key| read.contains(*key))
        .collect();
    assert!(both.is_empty(), "read and listed as unread: {both:?}");
}
