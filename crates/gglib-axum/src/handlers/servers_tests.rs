//! Tests for the start-server body: every key the CLI sends, read.

use super::*;
use gglib_core::contracts::http::daemon_bodies::SERVERS_START_CLI_FIELDS;

/// A JSON value for each name in the shared contract, none of them the value
/// its field has when the key is dropped.
///
/// The catch-all panics rather than defaulting, as the proxy body's does: a
/// name added to the contract with no sample here stops the suite and says
/// which.
fn sample_for(name: &str) -> serde_json::Value {
    use serde_json::json;
    match name {
        "id" => json!(7),
        "contextLength" => json!(8192),
        "port" => json!(9001),
        "jinja" => json!(false),
        "reasoningFormat" => json!("none"),
        "mtpDraftNMax" => json!(0),
        "mtpDraftPMin" => json!(0.5),
        "inferenceParams" => json!({ "temperature": 0.25 }),
        "mlock" => json!(true),
        other => panic!("no sample value for contract field `{other}` — add one"),
    }
}

/// Every key the CLI sends reaches the field the handler starts the model
/// from, with the value it was sent.
///
/// The destructuring is exhaustive on purpose, and the array's length is tied
/// to the contract's: a field the request grows without a contract name fails
/// to compile, and a contract name the daemon never grew a field for leaves
/// the two lengths apart.
#[test]
fn the_daemon_reads_every_key_the_cli_sends() {
    let body: serde_json::Map<String, serde_json::Value> = SERVERS_START_CLI_FIELDS
        .iter()
        .map(|name| ((*name).to_owned(), sample_for(name)))
        .collect();

    let StartServerBody { model_id, config } =
        serde_json::from_value(serde_json::Value::Object(body))
            .expect("the daemon must read the body the CLI sends");
    let StartServerRequest {
        context_length,
        port,
        jinja,
        reasoning_format,
        mtp_draft_n_max,
        mtp_draft_p_min,
        inference_params,
        mlock,
    } = config;

    let read: [(&str, bool); 9] = [
        ("id", model_id == Some(7)),
        ("contextLength", context_length == Some(8192)),
        ("port", port == Some(9001)),
        ("jinja", jinja == Some(false)),
        (
            "reasoningFormat",
            reasoning_format.as_deref() == Some("none"),
        ),
        ("mtpDraftNMax", mtp_draft_n_max == Some(0)),
        ("mtpDraftPMin", mtp_draft_p_min == Some(0.5)),
        (
            "inferenceParams",
            inference_params.and_then(|p| p.temperature) == Some(0.25),
        ),
        ("mlock", mlock),
    ];

    let mut names: Vec<&str> = read.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    let mut contract = SERVERS_START_CLI_FIELDS.to_vec();
    contract.sort_unstable();
    assert_eq!(names, contract, "the rows above and the contract disagree");

    for (name, arrived) in read {
        assert!(arrived, "{name} was dropped");
    }
}
