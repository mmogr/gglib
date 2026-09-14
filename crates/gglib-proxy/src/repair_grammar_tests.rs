//! Tests for a turn gglib's own grammar constrained: the request pipeline's
//! stage 6 installed a grammar and rewrote `tool_choice` to `"none"`.

use super::*;

const GRAMMAR: RepairTurn = RepairTurn {
    enabled: true,
    gglib_grammar: true,
};

/// The body stage 6 forwards: gglib's grammar, and `tool_choice: "none"`.
fn constrained_request() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model": "m",
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": [{"role": "user", "content": "read it"}],
        "grammar": "root ::= \"<tool_call>\" [^<]* \"</tool_call>\"",
        "tool_choice": "none",
        "tools": [{"type": "function", "function": {"name": "read_file", "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "max_lines": {"type": "integer"}},
            "required": ["path"]
        }}}]
    }))
    .unwrap()
}

fn one_call(arguments: &str) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"choices": [{"message": {"role": "assistant", "tool_calls": [{
            "type": "function",
            "function": {"name": "read_file", "arguments": arguments}
        }]}}]}),
    )
    .unwrap()
}

/// On a turn gglib's grammar constrained, a violation is drawn again under the
/// same grammar, non-streaming. The grammar and `tool_choice: "none"` stay,
/// since llama-server refuses a custom grammar beside any other choice.
#[test]
fn a_violation_under_gglibs_grammar_is_drawn_again_under_it() {
    let d = decide(&constrained_request(), &one_call(r#"{"path":42}"#), GRAMMAR);
    let Decision::Reissue { body, violations } = d else {
        panic!("expected a second draw, got {d:?}");
    };
    assert!(violations[0].contains("path"), "{violations:?}");

    let sent: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(sent["tool_choice"], "none");
    assert!(sent["grammar"].is_string(), "the grammar stays: {sent}");
    assert_eq!(sent["stream"], false);
    assert!(sent.get("stream_options").is_none(), "{sent}");
}

/// A conformant call under gglib's grammar is forwarded as it came.
#[test]
fn a_conformant_call_under_gglibs_grammar_is_forwarded() {
    assert_eq!(
        decide(
            &constrained_request(),
            &one_call(r#"{"path":"a"}"#),
            GRAMMAR
        ),
        Decision::Forward(Skipped::Conformant)
    );
}

/// The same body, when gglib did not install its grammar, carries a client's
/// own constraint, and is left alone.
#[test]
fn a_grammar_gglib_did_not_install_is_still_left_alone() {
    assert_eq!(
        decide(
            &constrained_request(),
            &one_call(r#"{"path":42}"#),
            RepairTurn::ON
        ),
        Decision::Forward(Skipped::AlreadyConstrained)
    );
}
