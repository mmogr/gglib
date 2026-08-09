//! Truncating a string for display without splitting a character.
//!
//! `&s[..n]` panics when `n` lands inside a multi-byte character, and `s.len()`
//! is bytes rather than characters — so the obvious spelling of "shorten this
//! for a log line" is a panic waiting for its first non-ASCII input. That is
//! not hypothetical: [`InferenceConfig::extract_client_sampling`] renders a
//! rejected client value into a log line, and a request body carrying
//! `{"temperature": "ααααα…"}` reached `&s[..40]` mid-character and took the
//! request task down with it.
//!
//! Two call sites wanted the same thing and only one of them got it right, so
//! the right answer lives here once rather than being re-derived per caller.
//!
//! # Byte budget, not character count
//!
//! Both functions take `max_bytes` and cut at or below it. A log line's
//! constraint is how much room it has, and a character budget cannot answer
//! that — 40 characters is 40 bytes of ASCII and 160 bytes of emoji. The cut
//! then moves *down* to the nearest character boundary, so the result is
//! always valid UTF-8 and never longer than asked for.
//!
//! [`InferenceConfig::extract_client_sampling`]: crate::domain::InferenceConfig::extract_client_sampling

use std::borrow::Cow;

/// Shorten `s` to at most `max_bytes`, cutting at a character boundary.
///
/// Returns `s` unchanged when it already fits. When it does not, the cut moves
/// down to the nearest boundary at or below `max_bytes`, so the result can be
/// shorter than the budget but is never longer and never invalid.
///
/// ```
/// use gglib_core::utils::text::truncate_at_char_boundary;
///
/// assert_eq!(truncate_at_char_boundary("hello", 10), "hello");
/// assert_eq!(truncate_at_char_boundary("hello", 3), "hel");
///
/// // "α" is two bytes, so a budget of 3 fits one of them, not one and a half.
/// assert_eq!(truncate_at_char_boundary("ααα", 3), "α");
/// ```
#[must_use]
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        s
    } else {
        &s[..s.floor_char_boundary(max_bytes)]
    }
}

/// [`truncate_at_char_boundary`], with an `…` marking what was cut.
///
/// Borrows when nothing was removed, so the common case of a string that
/// already fits allocates nothing. The ellipsis is appended only on an actual
/// truncation — a caller can therefore tell "this is the whole value" from
/// "there was more" by looking at the output, which is the point of printing it.
///
/// ```
/// use gglib_core::utils::text::truncate_with_ellipsis;
///
/// assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
/// assert_eq!(truncate_with_ellipsis("hello", 3), "hel…");
/// ```
#[must_use]
pub fn truncate_with_ellipsis(s: &str, max_bytes: usize) -> Cow<'_, str> {
    let head = truncate_at_char_boundary(s, max_bytes);
    if head.len() == s.len() {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(format!("{head}…"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_within_the_budget_is_returned_whole() {
        assert_eq!(truncate_at_char_boundary("hello", 5), "hello");
        assert_eq!(truncate_at_char_boundary("hello", 6), "hello");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn an_ascii_string_over_the_budget_is_cut_to_it() {
        assert_eq!(truncate_at_char_boundary("hello world", 5), "hello");
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
    }

    /// **The defect this module was extracted for.** `&s[..40]` panics when
    /// byte 40 lands inside a character, and `serde_json` does not escape
    /// non-ASCII, so any client string long enough reaches it.
    #[test]
    fn a_cut_that_lands_inside_a_character_moves_down_to_the_boundary() {
        // Each "α" is two bytes, so byte 40 of `"ααα…"` (a leading quote plus
        // pairs) is mid-character — exactly the shape that panicked.
        let rendered = format!("\"{}\"", "α".repeat(60));
        let out = truncate_at_char_boundary(&rendered, 40);

        assert!(out.len() <= 40, "must not exceed the budget: {}", out.len());
        assert_eq!(out.len(), 39, "one byte below, because α does not fit");
        assert!(rendered.starts_with(out), "must be a prefix");
    }

    /// Every budget across a multi-byte string must be safe, not just the one
    /// that happened to bite. Asserting no panic across the whole range is
    /// cheaper than reasoning about which offsets are boundaries.
    #[test]
    fn no_budget_can_split_a_character() {
        let s = "aα中𝄞bβ𝕏c"; // 1, 2, 3 and 4-byte characters, interleaved
        for budget in 0..=s.len() + 2 {
            let out = truncate_at_char_boundary(s, budget);
            assert!(out.len() <= budget.min(s.len()));
            assert!(s.starts_with(out));
        }
    }

    #[test]
    fn the_ellipsis_appears_only_when_something_was_cut() {
        assert!(!truncate_with_ellipsis("short", 40).contains('…'));
        assert!(truncate_with_ellipsis(&"x".repeat(41), 40).ends_with('…'));
    }

    /// A string that fits must not allocate — this runs on the request path.
    #[test]
    fn a_string_that_fits_is_borrowed_rather_than_copied() {
        assert!(matches!(
            truncate_with_ellipsis("short", 40),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    /// A budget of zero is a legitimate ask and must not panic or produce a
    /// lone ellipsis over nothing.
    #[test]
    fn a_zero_budget_yields_an_empty_head() {
        assert_eq!(truncate_at_char_boundary("αβγ", 0), "");
        assert_eq!(truncate_with_ellipsis("αβγ", 0), "…");
    }
}
