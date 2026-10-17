//! What each kind of search is: the tags it asks the Hub for, and which of
//! the hits that come back it keeps.

use gglib_core::ports::huggingface::HfModelKind;
use serde_json::Value;

/// The filter a search of `kind` asks the Hub with.
///
/// Both kinds ask for the `gguf` tag, and an image search the `text-to-image`
/// tag too; repeated tags must all be present. The Hub ignores `library=gguf`
/// (probed 2026-10-16: the same hits with or without it). A chat search asks
/// for no pipeline either: many chat repositories carry none, and the ones
/// that read images carry `image-text-to-text`, so `pipeline_tag=text-generation`
/// hid both. A chat search keeps its hits by their pipeline instead, in
/// [`keeps`].
pub(crate) const fn hub_filter(kind: HfModelKind) -> &'static str {
    match kind {
        HfModelKind::Chat => "filter=gguf",
        HfModelKind::Image => "filter=gguf&filter=text-to-image",
    }
}

/// Whether a search of `kind` keeps `hit`, one repository of the page the
/// Hub answered [`hub_filter`] with.
///
/// A chat search keeps a model that chats: its pipeline is one of
/// [`CHAT_PIPELINES`], or it names none, as many chat repositories don't. A
/// hit naming any other pipeline (speech, embeddings, pictures) is not one.
/// An image search keeps every hit: its tags already chose them.
pub(crate) fn keeps(kind: HfModelKind, hit: &Value) -> bool {
    match kind {
        HfModelKind::Chat => hit
            .get("pipeline_tag")
            .and_then(Value::as_str)
            .is_none_or(|pipeline| CHAT_PIPELINES.contains(&pipeline)),
        HfModelKind::Image => true,
    }
}

/// The pipelines of the models that chat: text in, text out, with or without
/// images, audio or video read alongside.
const CHAT_PIPELINES: &[&str] = &[
    "text-generation",
    "image-text-to-text",
    "any-to-any",
    "audio-text-to-text",
    "video-text-to-text",
];
