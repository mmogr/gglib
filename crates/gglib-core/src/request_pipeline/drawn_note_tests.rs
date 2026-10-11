//! Tests for [`super`]: when a request is given the note, and where.

use serde_json::{Value, json};

use super::super::tests_support::chars;
use super::super::{ModelContext, SamplingLayers, apply, shape_messages};
use super::*;
use crate::domain::ModelCapabilities;

/// What the drawing tool answers when it drew.
const DREW: &str =
    "Drew 1 image, 1024x1024 PNG, with flux in 76 s; the user can see it, you cannot.";

/// An assistant message that calls `tool` with call id `id`.
fn call(id: &str, tool: &str) -> Value {
    json!({ "role": "assistant", "tool_calls": [{
        "id": id,
        "type": "function",
        "function": { "name": tool, "arguments": "{\"prompt\":\"a fox\"}" },
    }]})
}

fn answer(id: &str, text: &str) -> Value {
    json!({ "role": "tool", "tool_call_id": id, "content": text })
}

fn said(role: &str, text: &str) -> Value {
    json!({ "role": role, "content": text })
}

/// A message sent with Draw, the call, the picture, the closing reply.
fn drawn_turn() -> Vec<Value> {
    vec![
        said("user", "draw a fox"),
        call("c1", DRAW_TOOL_NAME),
        answer("c1", DREW),
        said("assistant", "I asked for a red fox in the snow."),
    ]
}

/// The message after a drawn picture: the turn that drew, then "thanks".
fn after_a_draw() -> Vec<Value> {
    let mut messages = drawn_turn();
    messages.push(said("user", "thanks"));
    messages
}

fn body(messages: Vec<Value>) -> Value {
    json!({ "model": "m", "messages": Value::Array(messages) })
}

fn unknown() -> ModelContext {
    ModelContext::passthrough()
}

/// How many pieces of text in `body`'s messages hold the note.
fn notes(body: &Value) -> usize {
    let messages = body["messages"].as_array().unwrap();
    messages
        .iter()
        .flat_map(texts)
        .map(|text| text.matches(DRAWN_NOTE).count())
        .sum()
}

/// `body` is left byte for byte as it was, by the stage and by the pipeline's
/// message shaping.
fn assert_untouched(body: &Value) {
    let before = body.to_string();
    for placement in [Placement::SystemFront, Placement::NextUserMessage] {
        let mut shaped = body.clone();
        assert!(!place(&mut shaped, placement), "{placement:?}: {before}");
        assert_eq!(shaped.to_string(), before, "{placement:?}");
    }
    let mut shaped = body.clone();
    assert!(!shape_messages(&mut shaped, &unknown()), "{before}");
    assert_eq!(shaped.to_string(), before);
}

// ── When ──────────────────────────────────────────────────────────────────

/// The reported case, at the placement every request gets: the note is a
/// `system` message before everything else, once, and every message the
/// caller sent follows it unchanged. Shaped again, the body does not change.
#[test]
fn the_message_after_a_drawn_picture_carries_the_note_once_at_the_front() {
    let mut shaped = body(after_a_draw());
    assert!(shape_messages(&mut shaped, &unknown()));

    let mut expected = vec![said("system", DRAWN_NOTE)];
    expected.extend(after_a_draw());
    assert_eq!(shaped.to_string(), body(expected).to_string());
    assert_eq!(notes(&shaped), 1);

    let once = shaped.to_string();
    assert!(!shape_messages(&mut shaped, &unknown()), "shaped twice");
    assert_eq!(shaped.to_string(), once);
    for placement in [Placement::SystemFront, Placement::NextUserMessage] {
        assert!(!place(&mut shaped, placement), "{placement:?}");
        assert_eq!(shaped.to_string(), once);
    }
}

/// The note is the same bytes in the same place on the second and third
/// message after a draw: the front of the prompt does not move.
#[test]
fn the_note_is_in_the_same_place_on_each_later_message() {
    let mut second = after_a_draw();
    second.extend([said("assistant", "ok"), said("user", "and the tail?")]);
    let mut third = second.clone();
    third.extend([said("assistant", "bushy"), said("user", "nice")]);

    for placement in [Placement::SystemFront, Placement::NextUserMessage] {
        let shape = |messages: Vec<Value>| {
            let mut shaped = body(messages);
            assert!(place(&mut shaped, placement));
            assert_eq!(notes(&shaped), 1);
            shaped["messages"].as_array().unwrap().clone()
        };
        let (first, second, third) = (
            shape(after_a_draw()),
            shape(second.clone()),
            shape(third.clone()),
        );
        assert_eq!(second[..first.len()], first[..], "{placement:?}");
        assert_eq!(third[..second.len()], second[..], "{placement:?}");
    }
}

#[test]
fn a_chat_with_no_draw_is_sent_as_it_was() {
    assert_untouched(&body(vec![said("user", "hi")]));
    assert_untouched(&body(vec![
        said("system", "Be brief."),
        said("user", "what time is it?"),
        call("c1", "builtin:get_current_time"),
        answer("c1", "Drew a blank; the user can see the time is 10:00"),
        said("assistant", "10:00"),
        said("user", "thanks"),
    ]));
    assert_untouched(&json!({ "model": "m" }));
    assert_untouched(&json!({ "model": "m", "messages": "not an array" }));
}

/// Only the builtin's qualified name is the drawing tool. Another client's
/// own tool, an MCP server's and the `/mcp` gateway's spelling are not, even
/// when their answer reads as the drawing tool's does.
#[test]
fn a_tool_by_any_other_name_does_not_bring_the_note() {
    for theirs in [
        "generate_image",
        "3:generate_image",
        "3:builtin:generate_image",
        "builtin__generate_image",
        "builtin:generate_images",
        "Builtin:generate_image",
    ] {
        let mut messages = after_a_draw();
        messages[1] = call("c1", theirs);
        assert_untouched(&body(messages));
    }
}

/// A request that offers the drawing tool is the first of a message sent
/// with Draw: nothing is added, whatever the chat drew before.
#[test]
fn a_request_that_offers_the_drawing_tool_gets_nothing() {
    let mut offered = body(after_a_draw());
    offered["tools"] = json!([{ "type": "function", "function": { "name": DRAW_TOOL_NAME } }]);
    offered["tool_choice"] = json!("required");
    assert_untouched(&offered);
}

/// A request that offers other tools, the drawing tool not among them, is an
/// ordinary message of the chat and is told.
#[test]
fn a_request_that_offers_only_other_tools_is_told() {
    for theirs in ["builtin:get_current_time", "generate_image"] {
        let mut offered = body(after_a_draw());
        offered["tools"] = json!([{ "type": "function", "function": { "name": theirs } }]);
        assert!(shape_messages(&mut offered, &unknown()), "{theirs}");
        assert_eq!(notes(&offered), 1, "{theirs}");
    }
}

/// The closing request of a run sent with Draw ends with the tool's answer
/// and one of the drawing side's two instructions, and gets no note: in a
/// chat's first draw or its second, whatever the tool answered.
#[test]
fn a_draws_closing_request_gets_nothing() {
    for instruction in [DRAW_THEN_SAY, DRAW_THEN_SAY_FAILED] {
        let mut first = drawn_turn();
        first.truncate(3);
        first.push(said("user", instruction));
        assert_untouched(&body(first));

        for outcome in [DREW, "the image server stopped"] {
            let mut second = after_a_draw();
            second.extend([call("c2", DRAW_TOOL_NAME), answer("c2", outcome)]);
            second.push(said("user", instruction));
            assert_untouched(&body(second));
        }
    }
}

/// A history may hold the call and the model's words in one assistant
/// message, and a run whose closing reply never arrived left no words at
/// all. Either way the tool's answer is followed only by the person's next
/// message, as in a closing request, and that message is told.
#[test]
fn a_draw_with_no_reply_after_its_answer_still_brings_the_note() {
    let mut together = after_a_draw();
    together.remove(3);
    together[1]["content"] = json!("I asked for a red fox in the snow.");
    let mut unanswered = after_a_draw();
    unanswered.remove(3);

    for messages in [together, unanswered] {
        let mut expected = vec![said("system", DRAWN_NOTE)];
        expected.extend(messages.clone());
        let mut shaped = body(messages);
        assert!(shape_messages(&mut shaped, &unknown()));
        assert_eq!(shaped.to_string(), body(expected).to_string());
    }
}

/// A call the tool answered with an error drew nothing, and a call with no
/// answer at all is no draw: neither brings a note that says a picture
/// exists.
#[test]
fn a_chat_whose_draws_all_failed_gets_nothing() {
    for outcome in [
        "the image server stopped",
        "generate_image needs a prompt: describe the picture in detail",
        "Drew 1 image, 1024x1024 PNG, with flux in 76 s, but none could be stored, \
         so the user cannot see them: the store is full",
        "It says: Drew 1 image; the user can see it, you cannot.",
    ] {
        let mut messages = after_a_draw();
        messages[2] = answer("c1", outcome);
        assert_untouched(&body(messages));
    }

    let mut unanswered = after_a_draw();
    unanswered.remove(2);
    assert_untouched(&body(unanswered));

    let mut another_calls = after_a_draw();
    another_calls[2] = answer("c9", DREW);
    assert_untouched(&body(another_calls));
}

/// One picture drawn and a later draw that failed: the picture still exists,
/// and once the failed run has had its reply the chat is told.
#[test]
fn a_failed_draw_after_a_drawn_picture_still_brings_the_note() {
    let mut messages = after_a_draw();
    messages.extend([
        call("c2", DRAW_TOOL_NAME),
        answer("c2", "the image server stopped"),
        said("assistant", "The image server stopped."),
        said("user", "oh well"),
    ]);
    let mut shaped = body(messages);
    assert!(shape_messages(&mut shaped, &unknown()));
    assert_eq!(notes(&shaped), 1);
}

/// A call id used again by a later call of another tool is that tool's: its
/// answer is not the drawing tool's, however it reads.
#[test]
fn a_reused_call_id_belongs_to_its_latest_call() {
    let mut messages = after_a_draw();
    messages[2] = answer("c1", "the image server stopped");
    messages.extend([
        call("c1", "builtin:get_current_time"),
        answer("c1", DREW),
        said("assistant", "10:00"),
        said("user", "thanks"),
    ]);
    assert_untouched(&body(messages));
}

// ── Where: the front ──────────────────────────────────────────────────────

/// A chat with a system prompt has the note appended to it after a blank
/// line, in either content shape, and gains no message.
#[test]
fn at_the_front_the_note_follows_the_chats_own_system_prompt() {
    let mut messages = vec![said("system", "Be brief.")];
    messages.extend(after_a_draw());
    let mut shaped = body(messages.clone());
    assert!(place(&mut shaped, Placement::SystemFront));
    messages[0] = said("system", &format!("Be brief.\n\n{DRAWN_NOTE}"));
    assert_eq!(shaped.to_string(), body(messages.clone()).to_string());

    messages[0] = json!({ "role": "system", "content": [{ "type": "text", "text": "Be brief." }] });
    let mut shaped = body(messages.clone());
    assert!(place(&mut shaped, Placement::SystemFront));
    messages[0]["content"] = json!([
        { "type": "text", "text": "Be brief." },
        { "type": "text", "text": DRAWN_NOTE },
    ]);
    assert_eq!(shaped.to_string(), body(messages).to_string());
}

/// A model with no system role reads the note as the pipeline gives it any
/// system message: a `[System]:` user message, merged with the person's own
/// when the template wants strict turns.
#[test]
fn a_model_with_no_system_role_reads_the_note_as_a_user_message() {
    let strict = ModelContext {
        capabilities: ModelCapabilities::REQUIRES_STRICT_TURNS,
        ..ModelContext::passthrough()
    };
    let mut shaped = body(after_a_draw());
    assert!(shape_messages(&mut shaped, &strict));
    let first = &shaped["messages"][0];
    assert_eq!(first["role"], "user");
    assert_eq!(
        first["content"],
        format!("[System]: {DRAWN_NOTE}\n\ndraw a fox")
    );
    assert_eq!(notes(&shaped), 1);
    assert_eq!(shaped["messages"].as_array().unwrap().len(), 5);

    let once = shaped.to_string();
    shape_messages(&mut shaped, &strict);
    assert_eq!(shaped.to_string(), once, "shaped twice");
}

// ── Where: the next user message ──────────────────────────────────────────

/// The other placement: the note starts the first `user` message after the
/// picture, before the person's words, and nothing else changes.
#[test]
fn on_the_next_user_message_the_note_comes_before_the_persons_words() {
    let mut messages = vec![said("system", "Be brief.")];
    messages.extend(after_a_draw());
    messages.extend([said("assistant", "ok"), said("user", "and the tail?")]);
    let mut shaped = body(messages.clone());
    assert!(place(&mut shaped, Placement::NextUserMessage));

    messages[5] = said("user", &format!("{DRAWN_NOTE}\n\nthanks"));
    assert_eq!(shaped.to_string(), body(messages).to_string());
}

/// A user message made of parts gains the note as its first text part; its
/// image part is left as it was.
#[test]
fn on_a_user_message_of_parts_the_note_is_the_first_text_part() {
    let image = json!({ "type": "image_url", "image_url": { "url": "data:image/png;base64,AA" } });
    let mut messages = after_a_draw();
    messages[4] = json!({ "role": "user", "content": [
        image,
        { "type": "text", "text": "like this one?" },
    ]});
    let mut shaped = body(messages.clone());
    assert!(place(&mut shaped, Placement::NextUserMessage));

    messages[4]["content"] = json!([
        { "type": "text", "text": DRAWN_NOTE },
        image,
        { "type": "text", "text": "like this one?" },
    ]);
    assert_eq!(shaped.to_string(), body(messages).to_string());
}

/// With two pictures the note is on the message after the later one; with no
/// user message after the picture nothing is added.
#[test]
fn the_next_user_message_is_the_one_after_the_last_picture() {
    let mut messages = after_a_draw();
    messages.extend([
        call("c2", DRAW_TOOL_NAME),
        answer("c2", DREW),
        said("assistant", "A bluer fox."),
        said("user", "lovely"),
    ]);
    let mut shaped = body(messages.clone());
    assert!(place(&mut shaped, Placement::NextUserMessage));
    messages[8] = said("user", &format!("{DRAWN_NOTE}\n\nlovely"));
    assert_eq!(shaped.to_string(), body(messages).to_string());

    let ends_with_the_reply = body(drawn_turn());
    let before = ends_with_the_reply.to_string();
    let mut shaped = ends_with_the_reply;
    assert!(!place(&mut shaped, Placement::NextUserMessage));
    assert_eq!(shaped.to_string(), before);
}

// ── In the pipeline ───────────────────────────────────────────────────────

/// The whole pipeline adds the note, and its budget measures it: the same
/// chat without its drawn picture's note measures shorter by the note's
/// message.
#[test]
fn the_pipeline_adds_the_note_and_measures_it() {
    let layers = SamplingLayers::default();
    let mut told = body(after_a_draw());
    let with = apply(&mut told, &unknown(), &layers, Some(chars(100_000))).unwrap();
    assert_eq!(notes(&told), 1);
    assert_eq!(told["messages"][0], said("system", DRAWN_NOTE));

    let mut failed = after_a_draw();
    failed[2] = answer("c1", &"x".repeat(DREW.len()));
    let mut untold = body(failed);
    let without = apply(&mut untold, &unknown(), &layers, Some(chars(100_000))).unwrap();
    assert_eq!(notes(&untold), 0);

    let note = said("system", DRAWN_NOTE).to_string().len() + ",".len();
    assert_eq!(
        with.truncation.payload_chars_before,
        without.truncation.payload_chars_before + note
    );
}
