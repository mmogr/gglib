//! Which drawn picture a model that reads images is shown, and where.
//!
//! A picture the drawing tool made is kept on its tool message, by id
//! ([`AgentMessage::Tool`]'s `images`), and a tool message is sent to the
//! model as its text. So a model with a projector could not see the picture
//! it had just had drawn. [`shown_picture`] is the one rule for showing it:
//! the completion adapter asks it for each request it builds, reads the
//! picture from the store and writes it into that request alone. No row, no
//! event and no returned history holds what it adds.
//!
//! # The rule
//!
//! 1. Only for a model that reads images. Any other model is sent what it
//!    was sent before, and is never refused for a picture a tool made.
//! 2. Only a picture the drawing tool made: the images of a `tool` message
//!    that answers a call named exactly [`DRAW_TOOL_NAME`]. Another tool's
//!    images are never shown.
//! 3. Only the most recent one: the last image of the last such message.
//!    An older picture stays as its tool message's sentence.
//! 4. On the first `user` message after that tool message: the message the
//!    person sent next. It is the same message on every later request, so
//!    the prompt up to it does not change again until another picture is
//!    drawn; then the newer picture goes on the message after it, and the
//!    older message is sent as it was written.
//! 5. Nothing when no `user` message follows the picture yet, and nothing
//!    when the only one that does is the instruction a run sent with Draw
//!    gives its closing request ([`DRAW_THEN_SAY`],
//!    [`DRAW_THEN_SAY_FAILED`]): that request is not a message the person
//!    sent, and is not in the history afterwards.
//!
//! [`DRAWN_PICTURE_LINE`] goes before the picture, so the model reads what
//! the image is before the person's own words.

use crate::domain::AttachmentId;
use crate::domain::agent::AgentMessage;

use super::drawn_note::{DRAW_THEN_SAY, DRAW_THEN_SAY_FAILED, DRAW_TOOL_NAME};

/// The text part that goes before a drawn picture shown to the model.
///
/// It says whose the image is, so the model does not read it as something
/// the person attached or asked about, and it takes back the drawing tool's
/// "you cannot" for this one picture: the tool's answer is sent as it was
/// saved, and without this the model would be told it cannot see an image
/// it is looking at.
pub const DRAWN_PICTURE_LINE: &str = "(Added automatically, not written by the person: the \
    image below is the most recent picture the drawing tool made in this conversation, the \
    same one the person sees. You can see it now, although the tool's answer said you could \
    not. The person's own message follows.)";

/// The drawn picture a request shows its model, and the message it goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShownPicture<'a> {
    /// The index, in the request's messages, of the `user` message that
    /// carries the picture.
    pub at: usize,
    /// The picture.
    pub image: &'a AttachmentId,
}

/// The drawn picture `messages` show a model, by the [module](self)'s rule;
/// `None` when they show none. `image_input` is whether the model reads
/// images ([`crate::ports::ModelSummary::image_input`]).
#[must_use]
pub fn shown_picture(messages: &[AgentMessage], image_input: bool) -> Option<ShownPicture<'_>> {
    if !image_input {
        return None;
    }
    let (drawn_at, image) = last_drawn(messages)?;
    let mut later = messages.iter().enumerate().skip(drawn_at + 1);
    let (at, said) = later.find_map(|(index, message)| match message {
        AgentMessage::User { content, .. } => Some((index, content)),
        _ => None,
    })?;
    let closing = said.contains(DRAW_THEN_SAY) || said.contains(DRAW_THEN_SAY_FAILED);
    (!closing).then_some(ShownPicture { at, image })
}

/// The index of the last `tool` message that holds a picture the drawing
/// tool made, and that picture.
fn last_drawn(messages: &[AgentMessage]) -> Option<(usize, &AttachmentId)> {
    // Each call id the history has used so far, with whether its latest use
    // called the drawing tool: an id is reused from one turn to the next.
    let mut calls: Vec<(&str, bool)> = Vec::new();
    let mut picture = None;
    for (index, message) in messages.iter().enumerate() {
        match message {
            AgentMessage::Assistant { content } => {
                for call in &content.tool_calls {
                    calls.retain(|(used, _)| *used != call.id);
                    calls.push((&call.id, call.name == DRAW_TOOL_NAME));
                }
            }
            AgentMessage::Tool {
                tool_call_id,
                images,
                ..
            } => {
                let answers_a_draw = calls
                    .iter()
                    .any(|(used, draws)| used == tool_call_id && *draws);
                if let (true, Some(image)) = (answers_a_draw, images.last()) {
                    picture = Some((index, image));
                }
            }
            _ => {}
        }
    }
    picture
}

#[cfg(test)]
#[path = "drawn_picture_tests.rs"]
mod drawn_picture_tests;
