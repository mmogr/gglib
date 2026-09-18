//! The note the loop guard appends to a request it has decided not to refuse.
//!
//! # Where it goes, and why there
//!
//! The note's text is appended to the **content of the last message**, behind
//! a `[gglib loop guard]` marker, whatever that message's role is. It adds no
//! turn and no role, which is what makes it the one delivery every chat
//! template accepts.
//!
//! The obvious alternative — a trailing `system` message — was the first
//! choice and does not survive contact with real templates. Rendering the
//! chat templates llama.cpp bundles (at `e5a8d439`; 65 of the 69 compile in
//! minijinja) through the same minijinja
//! environment `gglib_gguf`'s template probe uses, against the two tails a
//! tripped request actually has, a trailing `system` message lands where it
//! was put in 63 of the 101 pairs that render at all. In the rest it
//! **raises** in 7 (Qwen3.5-4B: "System message must be at the beginning.";
//! `Ministral-3` and `Mistral-Nemo` on the Mistral side; `Apertus-8B`, which
//! is neither), is **hoisted to token 0** in 19 (the DeepSeek family, which
//! concatenates every system message into a prompt prefix — breaking the
//! cached prefix on every tripped turn, the exact failure `canonicalization`
//! exists to prevent — plus `tencent-Hy3`, `Solar-Open-100B` and rwkv-world's
//! chat tail), or is **silently dropped** in 10 (gpt-oss, `SmolLM3`,
//! `MiniMax-M1`, Nemotron-Nano-v2, Bielik). The remaining 2 are the
//! no-`tool`-branch pair below, where the whole history is lost and the note
//! survives. A raise is an HTTP 500 from llama-server where the guard used to
//! return a clean 400.
//! gglib's two capability flags identify none of these: llama.cpp probes
//! `supports_system_role` with the message at index 0, so Qwen3.5 and DeepSeek
//! both report that they support it.
//!
//! In-content delivery lands in place in 99 of the same 101. Its two failures
//! are templates with no branch for the `tool` role at all — Phi-3.5-mini and
//! rwkv-world — which drop the whole last message on an agentic tail, and the
//! note with it. That is a real limit, not an artefact: **a note inside the
//! last message shares that message's fate**, and `tool` is the role templates
//! most often omit. What bounds the damage is that a model behind such a
//! template never sees a tool *result* either, so it cannot run a tool loop
//! meaningfully with or without the note; and on a chat tail, where stagnation
//! trips, both templates render the note in place.
//!
//! The evidence is minijinja over llama.cpp's bundled templates, not
//! llama-server's own engine, and nothing here was run against a model. Of
//! the 69 templates, 4 do not compile in minijinja at all, so 65 were
//! rendered against 2 tails each — 130 pairs, of which 101 render on the
//! baseline. `loop_guard_note_templates_tests.rs` in `gglib-gguf` keeps the
//! four templates that broke the alternatives honest, with Phi-3.5-mini
//! pinned as the known drop; **the full table is not re-derivable from this
//! tree** — the harness that produced it was a throwaway, and the five
//! vendored templates are what survives of it.
//!
//! # Where it goes in the pipeline
//!
//! Applied in `server.rs` after the scan and before `body_for_retry` is
//! cloned, which puts it on the primary forward, the `UpstreamDead` retry, the
//! unary path and the repair re-issue, all of which derive from that body.
//!
//! One path carries the trip and never delivers the note: a request that also
//! exceeds the context budget is noted here and then refused as
//! `context_length_exceeded` inside the forward. The note is built and
//! appended first — this runs before `shape_request_body` — so what fails is
//! the sending, not the rendering. That is by construction the shape most
//! likely to trip the guard, and it is where the new default buys nothing.
//!
//! It also means the note's own characters are inside the payload the budget
//! is measured against: 245–486 for a loop, about 206 for stagnation. A
//! conversation that close to the ceiling is forwarded under `off` and
//! refused under the default.
//!
//! **After the scan** is load-bearing: the note cannot trip the guard that
//! wrote it. The sharper-sounding hazard — that `loop_guard`'s detectors reset
//! on any role that is not `tool`/`assistant`, so a note seen *before* the
//! scan would disable the guard rather than merely confuse it — is foreclosed
//! by the data flow rather than by the ordering, since the note is produced
//! *by* the scan's own step and cannot exist before it. It is not a second
//! reason, and a test cannot be written for it.
//!
//! The client never sees the note — it is added to the upstream request only,
//! and the model's answer comes back untouched — so it is never replayed, and
//! the guard therefore re-trips and re-notes on the next turn with the count
//! one higher. That steady state is the design: a model that ignores the note
//! forever is what the loop guard's log (`gglib proxy trips`) makes visible
//! across restarts — its trips climb while its distinct sessions do not.
//!
//! # What it costs
//!
//! One parse and one re-serialise of the request body, on tripped requests
//! only. `serde_json` here is built without `preserve_order`, so that round
//! trip also re-sorts every JSON object's keys: the *rendered prompt* ahead of
//! the note is unchanged, because every template reads `role` and `content` by
//! name, but the *bytes* are not, so tests assert `Value`-equality rather than
//! byte-identity.

use bytes::Bytes;
use serde_json::{Value, json};

use gglib_core::request_pipeline::append_text;

use crate::loop_guard::LoopGuardVerdict;

/// The marker every note carries, so a model can tell gglib's words from the
/// conversation's.
///
/// The same device gglib already uses when it rewrites a `system` message for
/// a model with no system role (`[System]: …`). It matters more here, because
/// on an agentic tail the last message is a `tool` result and on a chat tail
/// it is the person's own turn — without the marker the note would arrive as
/// their words.
pub(crate) const MARKER: &str = "[gglib loop guard]";

/// How much of a batch signature the note will echo.
///
/// The signature carries the client's own tool names verbatim (see
/// [`LoopGuardNote::for_verdict`]), so it is the one part of the note a client
/// controls. Long enough that a real batch of several tools is named in full —
/// a signature is `name:16 hex` per call — and short enough that a pathological
/// name cannot dominate the prompt.
const SIGNATURE_LIMIT: usize = 240;

/// `signature`, truncated on a character boundary if it is longer than
/// [`SIGNATURE_LIMIT`].
fn bound(signature: &str) -> std::borrow::Cow<'_, str> {
    if signature.chars().count() <= SIGNATURE_LIMIT {
        return std::borrow::Cow::Borrowed(signature);
    }
    let kept: String = signature.chars().take(SIGNATURE_LIMIT).collect();
    std::borrow::Cow::Owned(format!("{kept}…"))
}

/// A note the guard decided to send instead of a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopGuardNote {
    text: String,
}

impl LoopGuardNote {
    /// The note for a verdict, or `None` for [`LoopGuardVerdict::Pass`].
    ///
    /// Fixed text: the only things interpolated are the signature, the count
    /// and the threshold, all of which the verdict already carries. No message
    /// content and nothing a person typed reaches the model through here.
    ///
    /// The signature is **not** free of client input, and that matters more
    /// here than it did for the 400 body this replaces. It is
    /// `name:hash|name:hash…`: the arguments are hashed, but each tool *name*
    /// is verbatim off the wire, unbounded and unescaped. Under `refuse` it
    /// went into an error body the client reads back; under `note` it goes
    /// into the prompt. The client already owns the prompt, so this is not an
    /// escalation — but an unbounded echo inside gglib's own marked sentence
    /// is worth bounding, so the interpolated signature is truncated to
    /// [`SIGNATURE_LIMIT`] characters with an ellipsis.
    pub(crate) fn for_verdict(verdict: &LoopGuardVerdict) -> Option<Self> {
        let text = match verdict {
            LoopGuardVerdict::Pass => return None,
            LoopGuardVerdict::LoopDetected { signature } => {
                let signature = bound(signature);
                format!(
                    "{MARKER} This conversation has repeated the tool-call batch `{signature}` \
                     with nothing in between, and received the same result each time. Repeating \
                     it will not produce a different answer. Change approach, or tell the user \
                     what is blocking you."
                )
            }
            LoopGuardVerdict::StagnationDetected { count, max_steps } => format!(
                "{MARKER} This conversation has produced the same response {count} times, \
                 against a limit of {max_steps}. Repeating it will not produce a different \
                 answer. Change approach, or tell the user what is blocking you."
            ),
        };
        Some(Self { text })
    }

    /// The note's text, marker included.
    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Return `body` with the note appended to its last message.
    ///
    /// Fail-open, like the scan that produced it: a body that will not parse,
    /// or that carries no message to append to, is returned **unchanged**
    /// rather than rejected. The guard is protection, and a request it cannot
    /// annotate is still a request the client is entitled to.
    ///
    /// A last message whose `content` is neither a string nor an array — an
    /// assistant turn that is only tool calls, say — cannot carry the note, so
    /// a trailing `user` message carrying it is added instead. That shape is
    /// not one a request normally *ends* on, and the added turn is the
    /// second-best delivery rather than the measured one.
    pub(crate) fn append_to(&self, body: Bytes) -> Bytes {
        let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
            return body;
        };
        let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut) else {
            return body;
        };
        match messages.last_mut() {
            None => return body,
            Some(last) => {
                let appended = last
                    .get_mut("content")
                    .is_some_and(|content| append_text(content, &self.text));
                if !appended {
                    messages.push(json!({ "role": "user", "content": self.text }));
                }
            }
        }
        serde_json::to_vec(&value).map_or(body, Bytes::from)
    }
}

#[cfg(test)]
#[path = "loop_guard_note_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "loop_guard_note_bound_tests.rs"]
mod bound_tests;
