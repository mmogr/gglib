//! The chats the branching tests and the recorded cases are read from.

use super::{LineChat, LineRow};
use crate::domain::attachment::{AttachmentId, AttachmentInfo};
use crate::domain::chat::{Message, MessageRole};

/// A message of chat 7, saved at a time that grows with its id.
pub(super) fn message(id: i64, role: MessageRole, content: &str) -> Message {
    Message {
        id,
        conversation_id: 7,
        origin_id: None,
        role,
        content: content.to_owned(),
        created_at: format!("2026-10-09 08:00:{id:02}"),
        metadata: None,
        images: Vec::new(),
    }
}

/// The image a test names by `n`.
pub(super) fn image(n: usize) -> AttachmentId {
    AttachmentId::of(format!("image {n}").as_bytes())
}

/// `message` carrying the images numbered `0..count`.
pub(super) fn with_images(mut message: Message, count: usize) -> Message {
    message.images = (0..count)
        .map(|n| AttachmentInfo {
            id: image(n),
            mime: "image/png".to_owned(),
            width: 640,
            height: 480,
        })
        .collect();
    message
}

use MessageRole::{Assistant, Tool, User};

/// A chat of two turns each way, the second reply a tool call, its result
/// and the answer.
pub(super) fn kyoto() -> Vec<Message> {
    vec![
        message(1, User, "Plan a trip to Kyoto"),
        message(2, Assistant, "Day 1: temples\nDay 2: markets"),
        message(3, User, "Make it shorter"),
        message(4, Assistant, ""),
        message(5, Tool, "{\"weather\":\"rain\"}"),
        message(6, Assistant, "One day: Fushimi Inari, then Arashiyama."),
    ]
}

/// `kyoto` with one more question that nothing answers yet.
pub(super) fn unanswered() -> Vec<Message> {
    let mut path = kyoto();
    path.push(with_images(message(7, User, "And with kids?"), 1));
    path
}

/// A chat a model opened with a greeting, before any question.
pub(super) fn greeted() -> Vec<Message> {
    vec![
        message(1, Assistant, "Hello! Where to?"),
        message(2, User, "Kyoto"),
        message(3, Assistant, "Lovely."),
    ]
}

/// A message of a family's chat: `id`, a copy of `key`.
pub(super) fn row(id: i64, key: i64, role: MessageRole, text: &str) -> LineRow {
    LineRow {
        id,
        key,
        role,
        text: text.to_owned(),
        images: 0,
    }
}

/// Chat `id` of a family, last changed at second `changed`.
pub(super) fn chat(id: i64, changed: u32, rows: Vec<LineRow>) -> LineChat {
    LineChat {
        conversation_id: id,
        updated_at: format!("2026-10-09 09:00:{changed:02}"),
        rows,
    }
}

/// A family of four, grown from chat 10:
///
/// ```text
/// 10  Q1 A2 Q3 A4          the first chat
/// 20  Q1 A2 Q5 A6          Q3 edited: copies 1-2, asks Q5
/// 30  Q1 A2 Q3 A7          A4 regenerated: copies 1-3, answers A7
/// 40  Q1 A2                branched after A2, nothing since
/// ```
pub(super) fn family() -> Vec<LineChat> {
    let q1 = |id| row(id, 1, User, "Plan a trip to Kyoto");
    let a2 = |id| row(id, 2, Assistant, "Day 1: temples");
    let q3 = |id| row(id, 3, User, "Make it cheaper");
    vec![
        chat(
            10,
            4,
            vec![
                q1(1),
                a2(2),
                q3(3),
                row(4, 4, Assistant, "Hostels and buses."),
            ],
        ),
        chat(
            20,
            6,
            vec![
                q1(11),
                a2(12),
                row(13, 13, User, "Make it shorter"),
                row(14, 14, Assistant, "One day."),
            ],
        ),
        chat(
            30,
            7,
            vec![
                q1(21),
                a2(22),
                q3(23),
                row(24, 24, Assistant, "Hostels, buses, bentos."),
            ],
        ),
        chat(40, 8, vec![q1(31), a2(32)]),
    ]
}
