//! What an audio clip costs.
//!
//! A client may send a clip as an `input_audio` part, `{"data": <base64>,
//! "format": "wav" | "mp3"}` ([`super::content::audio_data`] is the one walk
//! over them), and the request goes to llama-server as it came. A model that
//! hears turns the clip into prompt tokens by its length in time, at a rate
//! its audio encoder sets, never by the length of the base64 that carries
//! it. So a clip of a few megabytes is not a few megabytes of text, and
//! [`super::measure`] does not count it as one.
//!
//! What a clip does cost is not known here: gglib holds no reading of any
//! audio encoder's rate, and the rate differs from one model family to the
//! next. Every clip is charged [`AUDIO_PART_TOKENS`], a stand-in and not an
//! estimate.

use serde_json::Value;

use super::content::audio_data;

/// The prompt tokens every audio clip is charged, whatever its length: a
/// stand-in, not an estimate.
///
/// No reading of what a clip costs exists to estimate it from. The figure is
/// kept small so that several clips fit the smallest context a model is
/// served at: a user's message is never trimmed, so a clip charged too much
/// is a refusal nothing can undo, while a clip charged too little is
/// forwarded and llama-server, which knows what it costs, judges whether it
/// fits.
pub(crate) const AUDIO_PART_TOKENS: usize = 512;

/// The base64 of every audio clip in the request's `messages`, history
/// included, in order.
pub(crate) fn request_audio_data(body: &Value) -> impl Iterator<Item = &str> {
    body.get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| message.get("content"))
        .flat_map(audio_data)
}

/// Whether any message of the request carries an audio clip.
#[must_use]
pub fn has_audio(body: &Value) -> bool {
    request_audio_data(body).next().is_some()
}
