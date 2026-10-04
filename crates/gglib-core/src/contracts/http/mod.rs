#![doc = include_str!("README.md")]
pub mod attachments;
pub mod daemon;
pub mod hf;

// Re-export for convenience
pub use hf::*;
// `daemon` stays qualified: its names (HEALTH_PATH, MODELS_LIST_PATH) are
// generic enough that a glob would make the call site ambiguous about which
// surface's contract it means.

/// The most bytes a request body may be on a route that takes images: the
/// proxy's `POST /v1/chat/completions` and `PUT /v1/runs/{id}`. An image
/// rides in the body as base64, a third larger than its file.
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// `raw` as one URL path segment: every byte but an RFC 3986 unreserved one
/// (`A-Z a-z 0-9 - . _ ~`) percent-encoded.
///
/// For a model identifier in a path. A model's name may hold `/`, `:`, `?`
/// or a space, and each would otherwise end the segment, start a query or be
/// refused by the client; encoded, the identifier reaches the route's `{name}`
/// whole, decoded, and nothing else.
#[must_use]
pub fn path_segment(raw: &str) -> String {
    raw.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
