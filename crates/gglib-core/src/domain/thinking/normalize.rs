//! Tag normalisation for variant thinking-tag formats.
//!
//! Converts all supported tag families to canonical `<think>` / `</think>`.

/// Normalize variant thinking-tag formats to the canonical `<think>` / `</think>`.
///
/// This function handles complete text (not streaming chunks).  For streaming
/// use cases, prefer [`super::ThinkingAccumulator`].
///
/// # Supported conversions
///
/// - `<seed:think>` → `<think>`, `</seed:think>` → `</think>`
/// - `<|START_THINKING|>` → `<think>`, `<|END_THINKING|>` → `</think>`
/// - `<reasoning>` → `<think>`, `</reasoning>` → `</think>`
pub fn normalize_thinking_tags(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut out = text.to_string();

    // Order matters: longest/most-specific first to avoid partial matches.
    // <|START_THINKING|> / <|END_THINKING|>
    out = replace_case_insensitive(&out, "<|START_THINKING|>", "<think>");
    out = replace_case_insensitive(&out, "<|END_THINKING|>", "</think>");

    // <seed:think> / </seed:think>
    out = replace_case_insensitive(&out, "<seed:think>", "<think>");
    out = replace_case_insensitive(&out, "</seed:think>", "</think>");

    // <reasoning> / </reasoning>
    out = replace_case_insensitive(&out, "<reasoning>", "<think>");
    out = replace_case_insensitive(&out, "</reasoning>", "</think>");

    out
}

/// Case-insensitive search-and-replace (no regex dependency).
fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    let needle_lower = needle.to_lowercase();
    let hay_lower = haystack.to_lowercase();
    let mut result = String::with_capacity(haystack.len());
    let mut start = 0;

    while let Some(pos) = hay_lower[start..].find(&needle_lower) {
        let abs = start + pos;
        result.push_str(&haystack[start..abs]);
        result.push_str(replacement);
        start = abs + needle.len();
    }
    result.push_str(&haystack[start..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_unchanged() {
        assert_eq!(normalize_thinking_tags(""), "");
    }

    #[test]
    fn standard_think_unchanged() {
        let input = "<think>Some thinking</think>Response";
        assert_eq!(normalize_thinking_tags(input), input);
    }

    #[test]
    fn seed_think_normalized() {
        let input = "<seed:think>Seed thinking</seed:think>Response";
        assert_eq!(
            normalize_thinking_tags(input),
            "<think>Seed thinking</think>Response"
        );
    }

    #[test]
    fn start_thinking_normalized() {
        let input = "<|START_THINKING|>Command R thinking<|END_THINKING|>Response";
        assert_eq!(
            normalize_thinking_tags(input),
            "<think>Command R thinking</think>Response"
        );
    }

    #[test]
    fn reasoning_normalized() {
        let input = "<reasoning>Deep reasoning</reasoning>Response";
        assert_eq!(
            normalize_thinking_tags(input),
            "<think>Deep reasoning</think>Response"
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            normalize_thinking_tags("<Reasoning>Mixed</Reasoning>"),
            "<think>Mixed</think>"
        );
        assert_eq!(
            normalize_thinking_tags("<SEED:THINK>Upper</SEED:THINK>"),
            "<think>Upper</think>"
        );
    }

    #[test]
    fn multiple_normalizations() {
        let input = "<reasoning>First</reasoning> and <seed:think>Second</seed:think>";
        assert_eq!(
            normalize_thinking_tags(input),
            "<think>First</think> and <think>Second</think>"
        );
    }
}
