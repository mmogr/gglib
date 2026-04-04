//! Complete-message parsing and embedding of thinking content.
//!
//! These functions work on full (non-streaming) messages.  For streaming,
//! see [`super::ThinkingAccumulator`].

use super::normalize::normalize_thinking_tags;
use super::types::ParsedThinkingContent;

/// Parse thinking content from a complete message.
///
/// The thinking block must appear at the very start of the text.
/// Tags are normalised before matching, so all four formats are accepted.
///
/// ```
/// use gglib_core::domain::thinking::parse_thinking_content;
///
/// let r = parse_thinking_content("<think>step 1</think>\nHello!");
/// assert_eq!(r.thinking.as_deref(), Some("step 1"));
/// assert_eq!(r.content, "Hello!");
/// ```
pub fn parse_thinking_content(text: &str) -> ParsedThinkingContent {
    let empty = ParsedThinkingContent {
        thinking: None,
        content: String::new(),
        duration_seconds: None,
    };

    if text.is_empty() {
        return empty;
    }

    let normalized = normalize_thinking_tags(text);

    // Match: ^<think(\s+duration="FLOAT")?\s*>(BODY)</think>\s*
    if let Some(after_tag) = strip_prefix_ci(&normalized, "<think") {
        // Parse optional attributes until '>'
        let after_tag = after_tag.trim_start();
        let (duration, rest) = parse_open_tag_attrs(after_tag);

        // `rest` starts right after the '>'
        if let Some(end_pos) = find_ci(rest, "</think>") {
            let thinking_raw = &rest[..end_pos];
            let thinking = thinking_raw.trim();
            let after_close = &rest[end_pos + "</think>".len()..];
            let content = after_close.trim_start().to_string();

            return ParsedThinkingContent {
                thinking: if thinking.is_empty() {
                    None
                } else {
                    Some(thinking.to_string())
                },
                content,
                duration_seconds: duration,
            };
        }
    }

    // No match — return original text as content.
    ParsedThinkingContent {
        thinking: None,
        content: text.to_string(),
        duration_seconds: None,
    }
}

/// Embed thinking content into a message using canonical `<think>` tags.
///
/// Round-trips with [`parse_thinking_content`].
///
/// ```
/// use gglib_core::domain::thinking::embed_thinking_content;
///
/// let msg = embed_thinking_content(Some("step 1"), "Answer", Some(3.5));
/// assert_eq!(msg, "<think duration=\"3.5\">step 1</think>\nAnswer");
/// ```
pub fn embed_thinking_content(
    thinking: Option<&str>,
    content: &str,
    duration_seconds: Option<f64>,
) -> String {
    match thinking {
        Some(t) if !t.is_empty() => {
            let dur_attr = duration_seconds.map_or_else(String::new, |d| format!(" duration=\"{d:.1}\""));
            format!("<think{dur_attr}>{t}</think>\n{content}")
        }
        _ => content.to_string(),
    }
}

/// Lightweight check for whether text begins with a thinking tag.
pub fn has_thinking_content(text: &str) -> bool {
    let trimmed = text.trim_start().to_lowercase();
    trimmed.starts_with("<think")
        || trimmed.starts_with("<reasoning")
        || trimmed.starts_with("<seed:think")
        || trimmed.starts_with("<|start_thinking|")
}

/// Format a duration for human display.
///
/// ```
/// use gglib_core::domain::thinking::format_thinking_duration;
///
/// assert_eq!(format_thinking_duration(5.5), "5.5s");
/// assert_eq!(format_thinking_duration(90.0), "1m 30s");
/// ```
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn format_thinking_duration(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{seconds:.1}s")
    } else {
        let minutes = (seconds / 60.0).floor() as u64;
        let remaining = (seconds % 60.0).round() as u64;
        format!("{minutes}m {remaining}s")
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Case-insensitive prefix strip: if `text` starts with `prefix` (case-insensitive),
/// returns the rest of `text` (preserving original case).
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let text_lower = text.to_lowercase();
    if text_lower.starts_with(&prefix.to_lowercase()) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

/// Case-insensitive find.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.to_lowercase();
    let n = needle.to_lowercase();
    h.find(&n)
}

/// Parse the attribute section of an opening `<think ...>` tag.
/// Input should be the text after `<think` and before/including `>`.
/// Returns `(duration_option, rest_after_closing_bracket)`.
fn parse_open_tag_attrs(s: &str) -> (Option<f64>, &str) {
    s.find('>').map_or((None, s), |gt| {
        let attrs = &s[..gt];
        let rest = &s[gt + 1..];
        (parse_duration_attr(attrs), rest)
    })
}

/// Extract the `duration` attribute value from a tag attribute string.
fn parse_duration_attr(attrs: &str) -> Option<f64> {
    let lower = attrs.to_lowercase();
    if let Some(pos) = lower.find("duration=\"") {
        let start = pos + "duration=\"".len();
        if let Some(end) = lower[start..].find('"') {
            return attrs[start..start + end].parse::<f64>().ok();
        }
    }
    None
}
