//! The session id of a request that names none: a hash of what its
//! conversation starts with.
//!
//! Split out of [`crate::canonicalization`], which is at its file budget.

use std::fmt::Write as _;
use std::sync::LazyLock;

use bytes::Bytes;
use gglib_core::domain::ChatMessage;
use gglib_core::request_pipeline::image_urls;
use regex::Regex;
use serde::Deserialize as _;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Matches dynamic IDE-injected lines at the start of a line (multiline mode).
///
/// The pattern captures the trailing newline (`\r?\n`) so `replace_all` removes
/// the entire line including its line ending.  Without consuming the newline a
/// matched line in the middle of the prompt would leave a double `\n\n`.
static DYNAMIC_LINE_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(Current date:|Current time:|Current terminal line count)[^\n]*(?:\r?\n|$)")
        .expect("hardcoded regex should always compile")
});

/// Number of leading digest bytes kept in [`derive_fallback_session_id`]'s
/// identifier (16 bytes = 128 bits — ample collision resistance for a cache
/// bucketing key that only needs fail-open behaviour on collision, not
/// cryptographic guarantees).
const FALLBACK_ID_DIGEST_BYTES: usize = 16;

/// Derive a stable, content-based session identifier for KV cache
/// save/restore when the caller did not supply an `X-Gglib-Session-Id`
/// header.
///
/// Hashes the system prompt together with the first user message: its text,
/// then the URL of each image it carries. Both are
/// stable for the entire life of one agent's conversation: `truncate_history`
/// (see `truncation.rs`) never modifies `system` messages or `user`-role
/// content, so this fingerprint doesn't drift as history grows. Different
/// agents (different system prompt) or different task instances of the same
/// agent (different first user message, or the same words about a different
/// image) land in different buckets without any client cooperation.
///
/// # Preconditions
///
/// None. It strips the dynamic lines itself, so the id is stable whether or
/// not canonicalisation ran, including when it is switched off via
/// [`DISABLE_CANONICALIZATION_ENV`](crate::canonicalization::DISABLE_CANONICALIZATION_ENV).
///
/// Returns `None` when the body has no usable `messages` array, or neither
/// a system prompt nor a first user message with text or an image is
/// present — callers should treat that the same as "no session available".
///
/// # Fail-open
///
/// A hash collision (two distinct conversations sharing an identical system
/// prompt *and* identical first user message) just means one restores the
/// other's cache; llama-server still re-syncs against whatever prefix
/// actually matches the incoming prompt, so the worst case is a wasted
/// restore/save, never a wrong answer.
pub(crate) fn derive_fallback_session_id(body: &Bytes) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let raw_messages = value.get("messages")?;
    let messages = Vec::<ChatMessage>::deserialize(raw_messages).ok()?;

    // Every dynamic line is stripped before hashing, not just coarsened.
    // `canonicalize_system_prompt` rounds the clock to the hour and leaves the
    // date alone, both of which still turn over eventually — and a fingerprint
    // that turns over costs the session its KV slot and its
    // `TokenCalibration` snapshot at the boundary. Removing them here
    // decouples the identity of a conversation from the clock entirely.
    let system_text = messages
        .iter()
        .find(|m| m.role == "system")
        .and_then(|m| m.content.clone())
        .map(|c| {
            DYNAMIC_LINE_PATTERNS
                .replace_all(&c.into_string(), "")
                .into_owned()
        })
        .unwrap_or_default();

    let first_user_text = messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.clone())
        .map(gglib_core::MessageContent::into_string)
        .unwrap_or_default();

    // The same message's images, read off the raw body: the typed message
    // above keeps only its text.
    let first_user_images = || {
        raw_messages
            .as_array()
            .into_iter()
            .flatten()
            .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
            .and_then(|m| m.get("content"))
            .into_iter()
            .flat_map(image_urls)
    };

    if system_text.is_empty() && first_user_text.is_empty() && first_user_images().next().is_none()
    {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(system_text.as_bytes());
    // Separator byte so ("ab", "c") and ("a", "bc") don't collide.
    hasher.update([0u8]);
    hasher.update(first_user_text.as_bytes());
    // Each image after the text, behind a separator of its own. A message
    // with no image adds nothing, so its id is what the text alone gives.
    for url in first_user_images() {
        hasher.update([0u8]);
        hasher.update(url.as_bytes());
    }
    let digest = hasher.finalize();

    let mut id = String::from("auto-");
    for byte in &digest[..FALLBACK_ID_DIGEST_BYTES] {
        let _ = write!(id, "{byte:02x}");
    }
    Some(id)
}

#[cfg(test)]
#[path = "fallback_session_tests.rs"]
mod fallback_session_tests;
