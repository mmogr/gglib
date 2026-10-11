//! An `OpenAI` history read as the loop's: each role, both content shapes,
//! images inline and by id, and what is refused.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::json;

use super::*;

fn id(bytes: &[u8]) -> AttachmentId {
    AttachmentId::of(bytes)
}

/// A history with every kind of message comes back as the loop's, and
/// those messages write the same wire the page's own history has.
#[test]
fn a_history_with_every_role_round_trips_into_the_loops_messages() {
    let pixels = b"\x89PNG-bytes".to_vec();
    let stored = id(b"already stored");
    let messages = json!([
        { "role": "system", "content": "Be brief." },
        { "role": "user", "content": [
            { "type": "text", "text": "what is this?" },
            { "type": "image_url", "image_url": {
                "url": format!("data:image/png;base64,{}", BASE64.encode(&pixels)) } },
            { "type": "image_url", "image_url": { "url": stored.as_str() } },
            { "type": "text", "text": "and this?" },
        ] },
        { "role": "assistant", "content": null, "tool_calls": [
            { "id": "c1", "type": "function", "function": {
                "name": "builtin:generate_image", "arguments": "{\"prompt\":\"a fox\"}" } },
        ] },
        { "role": "tool", "tool_call_id": "c1", "content": "Drew 1 image." },
        { "role": "assistant", "content": "I drew a fox." },
        { "role": "user", "content": "thanks" },
    ]);

    let turns = parse_openai_messages(&messages).unwrap();

    assert_eq!(
        inline_images(&turns).collect::<Vec<_>>(),
        [pixels.as_slice()]
    );
    let agent = into_agent_messages(turns, [id(&pixels)]).unwrap();
    let wire: Vec<serde_json::Value> = agent
        .iter()
        .map(|m| serde_json::to_value(m).unwrap())
        .collect();
    assert_eq!(
        wire,
        [
            json!({ "role": "system", "content": "Be brief." }),
            json!({ "role": "user", "content": "what is this?\nand this?",
                    "images": [id(&pixels), stored] }),
            json!({ "role": "assistant", "tool_calls": [
                { "id": "c1", "name": "builtin:generate_image",
                  "arguments": { "prompt": "a fox" } } ] }),
            json!({ "role": "tool", "tool_call_id": "c1", "content": "Drew 1 image." }),
            json!({ "role": "assistant", "content": "I drew a fox." }),
            json!({ "role": "user", "content": "thanks" }),
        ]
    );
}

/// A `developer` message is a system one; an assistant may say something
/// and call a tool at once; arguments that are an object, or do not read
/// as JSON, are kept.
#[test]
fn the_shapes_clients_send_are_read() {
    let messages = json!([
        { "role": "developer", "content": [{ "type": "text", "text": "Rules." }] },
        { "role": "assistant", "content": "Drawing.", "tool_calls": [
            { "id": "a", "function": { "name": "t", "arguments": { "n": 1 } } },
            { "id": "b", "function": { "name": "t", "arguments": "not json" } },
            { "id": "c", "function": { "name": "t" } },
        ] },
        { "role": "user", "content": [
            { "type": "image_url", "image_url": "data:image/jpeg;base64,AQID" } ] },
    ]);
    let turns = parse_openai_messages(&messages).unwrap();
    assert!(matches!(&turns[0], OpenAiTurn::System { content } if content == "Rules."));
    let OpenAiTurn::Assistant { text, tool_calls } = &turns[1] else {
        panic!("an assistant message");
    };
    assert_eq!(text.as_deref(), Some("Drawing."));
    let arguments: Vec<_> = tool_calls.iter().map(|c| c.arguments.clone()).collect();
    assert_eq!(arguments, [json!({ "n": 1 }), json!("not json"), json!({})]);
    let OpenAiTurn::User { content, images } = &turns[2] else {
        panic!("a user message");
    };
    assert_eq!(content, "");
    assert_eq!(images, &[OpenAiImage::Inline(vec![1, 2, 3])]);
}

/// What cannot be read is refused by its index, and never quoted.
#[test]
fn what_cannot_be_read_is_refused_by_its_index() {
    let refused = |messages: serde_json::Value| parse_openai_messages(&messages).unwrap_err();
    assert_eq!(refused(json!(null)), HistoryError::NoMessages);
    assert_eq!(refused(json!([])), HistoryError::NoMessages);
    assert_eq!(
        refused(
            json!([{ "role": "user", "content": "hi" }, { "role": "narrator", "content": "SECRET" }])
        ),
        HistoryError::UnknownRole { index: 1 }
    );
    let remote = json!([{ "role": "user", "content": [
        { "type": "image_url", "image_url": { "url": "https://example.com/SECRET.png" } } ] }]);
    assert_eq!(refused(remote), HistoryError::UnreadableImage { index: 0 });
    let not_base64 = json!([{ "role": "user", "content": [
        { "type": "image_url", "image_url": { "url": "data:image/png,rawbytes" } } ] }]);
    assert_eq!(
        refused(not_base64),
        HistoryError::UnreadableImage { index: 0 }
    );
    for (message, role) in [
        (json!({ "role": "user", "content": 7 }), "user"),
        (json!({ "role": "tool", "content": "x" }), "tool"),
        (json!({ "role": "assistant", "content": null }), "assistant"),
        (
            json!({ "role": "assistant", "tool_calls": [{ "id": "c" }] }),
            "assistant",
        ),
    ] {
        let error = refused(json!([{ "role": "system", "content": "s" }, message]));
        assert!(
            matches!(error, HistoryError::Unreadable { index: 1, role: r, .. } if r == role),
            "{error:?}"
        );
        assert!(!error.to_string().contains("SECRET"));
    }
}

/// An assistant message that said nothing, as a blank reply is kept, is
/// read as its empty text rather than refusing the whole history.
#[test]
fn a_blank_assistant_reply_is_kept_as_empty_text() {
    let messages = json!([{ "role": "assistant", "content": "", "tool_calls": [] }]);
    let turns = parse_openai_messages(&messages).unwrap();
    let OpenAiTurn::Assistant { text, tool_calls } = &turns[0] else {
        panic!("an assistant message");
    };
    assert_eq!((text.as_deref(), tool_calls.len()), (Some(""), 0));
}

/// An inline image with no id to name it is an error, not a dropped image.
#[test]
fn an_inline_image_with_no_id_is_refused() {
    let messages = json!([{ "role": "user", "content": [
        { "type": "image_url", "image_url": { "url": "data:image/png;base64,AQID" } } ] }]);
    let turns = parse_openai_messages(&messages).unwrap();
    assert_eq!(
        into_agent_messages(turns, []).unwrap_err(),
        HistoryError::ImageNotStored
    );
}

/// A stored image keeps its own id wherever it stands among inline ones,
/// and the inline ones take the stored ids in order, across messages.
#[test]
fn stored_and_inline_images_each_land_in_their_place_across_messages() {
    let (first, second) = (b"first inline".to_vec(), b"second inline".to_vec());
    let stored = id(b"already stored");
    let data = |bytes: &[u8]| format!("data:image/png;base64,{}", BASE64.encode(bytes));
    let messages = json!([
        { "role": "user", "content": [
            { "type": "image_url", "image_url": { "url": stored.as_str() } },
            { "type": "image_url", "image_url": { "url": data(&first) } },
        ] },
        { "role": "user", "content": [
            { "type": "image_url", "image_url": { "url": data(&second) } },
        ] },
    ]);
    let turns = parse_openai_messages(&messages).unwrap();
    assert_eq!(
        inline_images(&turns).collect::<Vec<_>>(),
        [first.as_slice(), second.as_slice()]
    );

    let agent = into_agent_messages(turns, [id(&first), id(&second)]).unwrap();

    let images: Vec<_> = agent
        .iter()
        .map(|message| match message {
            AgentMessage::User { images, .. } => images.clone(),
            other => panic!("a user message, not {other:?}"),
        })
        .collect();
    assert_eq!(images, [vec![stored, id(&first)], vec![id(&second)]]);
}
