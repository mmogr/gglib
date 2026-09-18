//! What real chat templates do with a note appended to the last message.
//!
//! The loop guard's note is delivered by appending its text to the content of
//! the last message ([`gglib_core::request_pipeline::append_text`]), because a
//! trailing `system` message — the obvious alternative — raises, is hoisted to
//! the front of the prompt, or is silently dropped depending on the template.
//! That finding came from rendering all 69 chat templates llama.cpp bundles;
//! this keeps the four that decided it honest, plus the one known drop, so a
//! template update or a change to the delivery cannot quietly undo it.
//!
//! Vendored from llama.cpp `e5a8d439`, `models/templates/`, byte for byte.
//!
//! **Scope.** This is minijinja with the same lenient environment
//! `template_probe` uses, not llama-server's own Jinja engine, and the
//! templates are llama.cpp's copies rather than each model's shipped one.
//! Nothing here was run against a model. What it does establish is the part
//! that no engine can differ on: a template that calls `raise_exception` on a
//! trailing `system` message raises, and one with no branch for a role drops
//! that message whole.

use minijinja::context;
use serde_json::{Value, json};

use gglib_core::request_pipeline::append_text;

use super::template_probe::build_env;

const QWEN35: &str = include_str!("testdata/loop_guard_note/Qwen3.5-4B.jinja");
const MISTRAL_NEMO: &str =
    include_str!("testdata/loop_guard_note/mistralai-Mistral-Nemo-Instruct-2407.jinja");
const DEEPSEEK_V31: &str = include_str!("testdata/loop_guard_note/deepseek-ai-DeepSeek-V3.1.jinja");
const GPT_OSS: &str = include_str!("testdata/loop_guard_note/openai-gpt-oss-120b.jinja");
const PHI_35_MINI: &str =
    include_str!("testdata/loop_guard_note/microsoft-Phi-3.5-mini-instruct.jinja");

/// A sentinel that no template emits on its own, standing in for the note's
/// text. The note's actual wording and its `[gglib loop guard]` marker are the
/// proxy's, and `loop_guard_note_tests.rs` there pins them; what is under test
/// here is only where the text ends up.
const NOTE: &str = "ZZNOTEZZ";

/// A sentinel marking the end of the history, so "after everything the client
/// sent" can be asserted rather than merely "present".
const LAST: &str = "ZZLASTZZ";

/// The agentic continuation a tripped tool loop really has: the client ran the
/// calls, appended the results, and asked the model to carry on.
fn agentic_tail() -> Vec<Value> {
    vec![
        json!({ "role": "system", "content": "be helpful" }),
        json!({ "role": "user", "content": "fix the build" }),
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "abcdefghi",
                "type": "function",
                // An object, not a JSON string: llama.cpp normalises a
                // tool call's arguments before rendering, and Qwen3.5's
                // template iterates them as pairs.
                "function": { "name": "write_file", "arguments": { "path": "src/main.rs" } }
            }]
        }),
        json!({ "role": "tool", "name": "write_file", "tool_call_id": "abcdefghi",
                "content": format!("1 file changed {LAST}") }),
    ]
}

/// A chat turn, which is the shape a stagnation trip has.
fn chat_tail() -> Vec<Value> {
    vec![
        json!({ "role": "system", "content": "be helpful" }),
        json!({ "role": "user", "content": format!("carry on {LAST}") }),
    ]
}

/// Append the note the way the proxy does — to the content of the last message
/// — and render `template` over the result.
fn render_with_note(template: &str, mut messages: Vec<Value>) -> Result<String, String> {
    let last = messages.last_mut().expect("a last message");
    let content = last.get_mut("content").expect("content");
    assert!(
        append_text(content, NOTE),
        "the tails here all carry text content"
    );

    let env = build_env(template).map_err(|e| format!("compile: {e}"))?;
    let tmpl = env.get_template("probe").map_err(|e| e.to_string())?;
    tmpl.render(context! {
        messages => messages,
        add_generation_prompt => true,
        bos_token => "<s>",
        eos_token => "</s>",
    })
    .map_err(|e| format!("{e}"))
}

/// The note must be present, and after everything the client sent.
fn assert_note_last(template: &str, messages: Vec<Value>, name: &str) {
    let out = match render_with_note(template, messages) {
        Ok(out) => out,
        Err(e) => panic!("{name} failed to render: {e}"),
    };
    let note_at = out
        .find(NOTE)
        .unwrap_or_else(|| panic!("{name} dropped the note: {out}"));
    let last_at = out
        .find(LAST)
        .unwrap_or_else(|| panic!("{name} dropped the last message: {out}"));
    assert!(
        note_at > last_at,
        "{name} put the note before the history it belongs after: {out}"
    );
}

#[test]
fn the_four_templates_that_broke_the_alternatives_render_the_note_in_place() {
    // Qwen3.5 raises on a trailing `system` message ("System message must be
    // at the beginning"), Mistral-Nemo raises on role alternation, DeepSeek
    // V3.1 hoists every system message to token 0, and gpt-oss drops one
    // silently. None of them can object to text inside a turn it has already
    // accepted.
    for (name, template) in [
        ("Qwen3.5-4B", QWEN35),
        ("Mistral-Nemo-Instruct-2407", MISTRAL_NEMO),
        ("DeepSeek-V3.1", DEEPSEEK_V31),
        ("gpt-oss-120b", GPT_OSS),
    ] {
        assert_note_last(template, chat_tail(), name);
    }
}

#[test]
fn the_agentic_tail_carries_the_note_too_on_templates_that_render_tool_results() {
    for (name, template) in [
        ("Qwen3.5-4B", QWEN35),
        ("DeepSeek-V3.1", DEEPSEEK_V31),
        ("gpt-oss-120b", GPT_OSS),
    ] {
        assert_note_last(template, agentic_tail(), name);
    }
}

#[test]
fn phi_3_5_mini_is_the_known_drop_and_only_on_a_tool_tail() {
    // The limit this delivery accepts, pinned rather than left to be
    // rediscovered: a note inside the last message shares that message's
    // fate, and this template has no branch for the `tool` role at all, so an
    // agentic tail is dropped whole and the note with it. What bounds the
    // damage is in the same fact — a model behind this template never sees a
    // tool *result* either, so it cannot run a tool loop meaningfully with or
    // without the note.
    assert!(
        !PHI_35_MINI.contains("tool"),
        "this template gained a `tool` branch upstream; the drop may be fixed"
    );
    let out = render_with_note(PHI_35_MINI, agentic_tail()).expect("renders");
    assert!(
        !out.contains(NOTE),
        "the known drop no longer drops; update the module docs and the ADR note: {out}"
    );

    // On a chat tail — where stagnation trips — the same template renders it.
    assert_note_last(PHI_35_MINI, chat_tail(), "Phi-3.5-mini [chat]");
}

#[test]
fn a_trailing_system_message_still_fails_the_way_the_probe_found() {
    // The evidence for the delivery, kept executable: if this ever passes,
    // the reason the note is not a `system` message has changed.
    let mut messages = chat_tail();
    messages.push(json!({ "role": "system", "content": NOTE }));
    let env = build_env(QWEN35).expect("compiles");
    let err = env
        .get_template("probe")
        .expect("template")
        .render(context! {
            messages => messages,
            add_generation_prompt => true,
            bos_token => "<s>",
            eos_token => "</s>",
        })
        .expect_err("Qwen3.5 raises on a trailing system message");
    assert!(
        format!("{err}").contains("System message must be at the beginning"),
        "unexpected error: {err}"
    );
}
