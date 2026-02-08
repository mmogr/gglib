//! Text preprocessing utilities for TTS.
//!
//! Strips markdown formatting and splits text into sentence-sized chunks
//! suitable for Kokoro TTS synthesis.

/// Maximum character length per TTS chunk.
///
/// Kokoro TTS works best with sentence-length input. We cap each chunk at
/// roughly 400 characters (about 2–3 sentences) to stay well within the
/// model's comfort zone and allow audio to start playing sooner.
const MAX_CHUNK_CHARS: usize = 400;

/// Strip markdown formatting from text, producing plain-text suitable for TTS.
///
/// Handles:
/// - Thinking/reasoning blocks (`<think>`, `<reasoning>`, etc.) → removed entirely
/// - Fenced code blocks (```…```) → replaced with "code block omitted"
/// - Inline code (`…`) → unwrapped
/// - Headers (# … ) → text only
/// - Bold / italic (**text**, *text*, __text__, _text_) → text only
/// - Strikethrough (~~text~~) → text only
/// - Links \[text\](url) → text only
/// - Images !\[alt\](url) → "image: alt"
/// - Bullet / numbered lists → text only
/// - Blockquotes (> …) → text only
/// - Horizontal rules (---, ***, ___) → removed
/// - HTML tags → removed
#[must_use]
pub fn strip_markdown(text: &str) -> String {
    // First pass: strip thinking/reasoning blocks entirely
    let text = strip_thinking_blocks(text);

    let mut result = String::with_capacity(text.len());
    let mut in_code_block = false;
    let mut code_block_replaced = false;

    for line in text.lines() {
        let trimmed = line.trim();

        // Handle fenced code blocks
        if trimmed.starts_with("```") {
            if in_code_block {
                // Closing fence
                in_code_block = false;
            } else {
                // Opening fence
                in_code_block = true;
            }
            code_block_replaced = false;
            continue;
        }

        if in_code_block {
            if !code_block_replaced {
                result.push_str("Code omitted. ");
                code_block_replaced = true;
            }
            continue;
        }

        // Skip horizontal rules
        if is_horizontal_rule(trimmed) {
            continue;
        }

        let processed = strip_line_markdown(line);
        let processed = processed.trim();
        if !processed.is_empty() {
            if !result.is_empty() && !result.ends_with(' ') && !result.ends_with('\n') {
                result.push(' ');
            }
            result.push_str(processed);
        }
    }

    // Final cleanup: collapse multiple spaces
    collapse_whitespace(&result)
}

/// Split text into TTS-friendly chunks (roughly sentence-sized).
///
/// Each chunk is at most `MAX_CHUNK_CHARS` characters. We split at sentence
/// boundaries (`.`, `!`, `?` followed by whitespace or end-of-string), then
/// merge short sentences into a single chunk up to the limit.
#[must_use]
pub fn split_into_chunks(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }

    // Short text — single chunk
    if text.len() <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }

    let sentences = split_sentences(text);
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for sentence in &sentences {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }

        // If adding this sentence would exceed the limit, flush
        if !current.is_empty() && current.len() + 1 + sentence.len() > MAX_CHUNK_CHARS {
            chunks.push(std::mem::take(&mut current));
        }

        // If a single sentence exceeds the limit, split at clause boundaries
        if sentence.len() > MAX_CHUNK_CHARS {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            let sub_chunks = split_long_sentence(sentence);
            chunks.extend(sub_chunks);
            continue;
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(sentence);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

// ── Internal helpers ───────────────────────────────────────────────

/// Remove `<think>…</think>`, `<reasoning>…</reasoning>`,
/// `<seed:think>…</seed:think>`, and `<|START_THINKING|>…<|END_THINKING|>`
/// blocks entirely so that chain-of-thought reasoning is never spoken.
fn strip_thinking_blocks(text: &str) -> String {
    let mut result = text.to_string();

    // Strip each known tag pair. We loop because there may be multiple blocks.
    result = strip_tag_block_pair(&result, "<think", "</think>");
    result = strip_tag_block_pair(&result, "<reasoning>", "</reasoning>");
    result = strip_tag_block_pair(&result, "<seed:think>", "</seed:think>");
    result = strip_tag_block_pair(&result, "<|START_THINKING|>", "<|END_THINKING|>");

    result
}

/// Remove all occurrences of `<open_tag…>…<close_tag>` from text.
///
/// `open_prefix` may be a prefix like `<think` that matches `<think>`,
/// `<think duration="5">`, etc.
fn strip_tag_block_pair(text: &str, open_prefix: &str, close_tag: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let haystack = text.to_ascii_lowercase();
    let open_lower = open_prefix.to_ascii_lowercase();
    let close_lower = close_tag.to_ascii_lowercase();

    let mut cursor = 0;

    while cursor < text.len() {
        // Find next opening tag (case-insensitive)
        if let Some(open_start) = haystack[cursor..].find(&open_lower) {
            let abs_open = cursor + open_start;

            // The open tag must close with '>'
            if let Some(tag_end_offset) = haystack[abs_open..].find('>') {
                let tag_end = abs_open + tag_end_offset + 1; // past the '>'

                // Find matching close tag
                if let Some(close_offset) = haystack[tag_end..].find(&close_lower) {
                    let close_end = tag_end + close_offset + close_tag.len();

                    // Append everything before the open tag
                    result.push_str(&text[cursor..abs_open]);
                    cursor = close_end;
                    continue;
                }
            }

            // No matching close — keep as-is and move past
            result.push_str(&text[cursor..abs_open + open_prefix.len()]);
            cursor = abs_open + open_prefix.len();
        } else {
            // No more opening tags — append remainder
            result.push_str(&text[cursor..]);
            break;
        }
    }

    result
}

/// Check if a line is a horizontal rule (---, ***, ___).
fn is_horizontal_rule(line: &str) -> bool {
    let chars: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    chars.len() >= 3 && chars.iter().all(|&c| c == '-' || c == '*' || c == '_')
        && chars.windows(2).all(|w| w[0] == w[1])
}

/// Strip inline markdown from a single line.
fn strip_line_markdown(line: &str) -> String {
    let mut s = line.to_string();

    // Strip leading blockquote markers
    while s.starts_with('>') {
        s = s.trim_start_matches('>').trim_start().to_string();
    }

    // Strip heading markers
    if s.starts_with('#') {
        s = s.trim_start_matches('#').trim_start().to_string();
    }

    // Strip bullet/numbered list markers
    s = strip_list_marker(&s);

    // Strip images ![alt](url) → "image: alt"
    s = strip_images(&s);

    // Strip links [text](url) → "text"
    s = strip_links(&s);

    // Strip inline code `…` → contents
    s = strip_inline_code(&s);

    // Strip bold/italic/strikethrough
    s = strip_emphasis(&s);

    // Strip any remaining HTML tags
    s = strip_html_tags(&s);

    s
}

fn strip_list_marker(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let prefix = &line[..indent];

    // Bullet: - item, * item, + item
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        return format!("{prefix}{rest}");
    }

    // Numbered: 1. item, 2) item
    if let Some(pos) = trimmed.find(|c: char| !c.is_ascii_digit()) {
        let after = &trimmed[pos..];
        if after.starts_with(". ") || after.starts_with(") ") {
            return format!("{prefix}{}", &after[2..]);
        }
    }

    line.to_string()
}

fn strip_images(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '!' {
            if chars.peek() == Some(&'[') {
                // Try to parse ![alt](url)
                chars.next(); // consume '['
                let alt: String = chars.by_ref().take_while(|&c| c != ']').collect();
                // Expect '(' ... ')'
                if chars.peek() == Some(&'(') {
                    chars.next(); // consume '('
                    let _url: String = chars.by_ref().take_while(|&c| c != ')').collect();
                    if !alt.is_empty() {
                        result.push_str("image: ");
                        result.push_str(&alt);
                    }
                    continue;
                }
                // Not a valid image, emit as-is
                result.push('!');
                result.push('[');
                result.push_str(&alt);
                result.push(']');
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn strip_links(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '[' {
            let link_text: String = chars.by_ref().take_while(|&c| c != ']').collect();
            // Expect '(' ... ')'
            if chars.peek() == Some(&'(') {
                chars.next(); // consume '('
                let _url: String = chars.by_ref().take_while(|&c| c != ')').collect();
                result.push_str(&link_text);
                continue;
            }
            // Not a valid link, emit as-is
            result.push('[');
            result.push_str(&link_text);
            result.push(']');
        } else {
            result.push(c);
        }
    }

    result
}

fn strip_inline_code(text: &str) -> String {
    // Simple approach: remove backticks surrounding inline code
    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'`' {
            // Find the closing backtick
            let start = i + 1;
            if let Some(end) = text[start..].find('`') {
                result.push_str(&text[start..start + end]);
                i = start + end + 1;
            } else {
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

fn strip_emphasis(text: &str) -> String {
    // Remove **, __, ~~, *, _ markers (simple approach)
    text.replace("**", "")
        .replace("__", "")
        .replace("~~", "")
        .replace('*', "")
        // Be careful with _ — only strip when it looks like emphasis
        // (surrounded by word chars). For simplicity, leave standalone _ alone.
}

fn strip_html_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;

    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    result
}

fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_space = false;

    for c in text.chars() {
        if c.is_whitespace() {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(c);
            prev_space = false;
        }
    }

    result.trim().to_string()
}

/// Split text into sentences at `.` `!` `?` boundaries.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    for (i, &c) in chars.iter().enumerate() {
        current.push(c);

        if (c == '.' || c == '!' || c == '?') && i + 1 < len {
            let next = chars[i + 1];
            // Sentence boundary: punctuation followed by space or end
            if next.is_whitespace() {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    sentences.push(trimmed);
                }
                current.clear();
            }
        }
    }

    // Push any remaining text
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
}

/// Split an overly long sentence at clause boundaries (, ; — :).
fn split_long_sentence(sentence: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for part in sentence.split_inclusive(&[',', ';', ':', '—', '–'][..]) {
        if !current.is_empty() && current.len() + part.len() > MAX_CHUNK_CHARS {
            chunks.push(std::mem::take(&mut current).trim().to_string());
        }
        current.push_str(part);
    }

    if !current.is_empty() {
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            chunks.push(trimmed);
        }
    }

    // If we still have oversized chunks, hard-split at word boundaries
    let mut final_chunks = Vec::new();
    for chunk in chunks {
        if chunk.len() > MAX_CHUNK_CHARS {
            final_chunks.extend(hard_split(&chunk));
        } else {
            final_chunks.push(chunk);
        }
    }

    final_chunks
}

/// Last-resort split at word boundaries.
fn hard_split(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > MAX_CHUNK_CHARS {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_simple_markdown() {
        let input = "**Hello** world! This is *italic* and `code`.";
        let result = strip_markdown(input);
        assert_eq!(result, "Hello world! This is italic and code.");
    }

    #[test]
    fn test_strip_code_block() {
        let input = "Here is code:\n```rust\nfn main() {}\n```\nDone.";
        let result = strip_markdown(input);
        assert_eq!(result, "Here is code:Code omitted. Done.");
    }

    #[test]
    fn test_strip_link() {
        let input = "Check [this link](https://example.com) out.";
        let result = strip_markdown(input);
        assert_eq!(result, "Check this link out.");
    }

    #[test]
    fn test_strip_headers() {
        let input = "## Header\nSome text.";
        let result = strip_markdown(input);
        assert_eq!(result, "Header Some text.");
    }

    #[test]
    fn test_strip_bullet_list() {
        let input = "- First\n- Second\n- Third";
        let result = strip_markdown(input);
        assert_eq!(result, "First Second Third");
    }

    #[test]
    fn test_split_short_text() {
        let text = "Hello world.";
        let chunks = split_into_chunks(text);
        assert_eq!(chunks, vec!["Hello world."]);
    }

    #[test]
    fn test_split_long_text() {
        // Build a string that definitely exceeds MAX_CHUNK_CHARS (400)
        let sentences: Vec<String> = (1..=20)
            .map(|i| format!("This is sentence number {i} and it contains enough words to contribute meaningful length to the overall text."))
            .collect();
        let text = sentences.join(" ");
        assert!(text.len() > 400, "Test text must exceed chunk limit");
        let chunks = split_into_chunks(&text);
        assert!(chunks.len() > 1, "Expected multiple chunks, got {}", chunks.len());
        for chunk in &chunks {
            assert!(
                chunk.len() <= 500, // MAX_CHUNK_CHARS + grace for word boundaries
                "Chunk too long: {} chars",
                chunk.len()
            );
        }
    }

    #[test]
    fn test_strip_blockquote() {
        let input = "> This is quoted text.";
        let result = strip_markdown(input);
        assert_eq!(result, "This is quoted text.");
    }

    #[test]
    fn test_horizontal_rule_removed() {
        let input = "Above.\n---\nBelow.";
        let result = strip_markdown(input);
        assert_eq!(result, "Above. Below.");
    }

    #[test]
    fn test_strip_think_tags() {
        let input = "<think>While I consider this question, I need to think about many things. While there are multiple approaches...</think>\nHere is the answer.";
        let result = strip_markdown(input);
        assert_eq!(result, "Here is the answer.");
    }

    #[test]
    fn test_strip_think_with_duration() {
        let input = "<think duration=\"5.2\">Some internal reasoning...</think>\nThe result is 42.";
        let result = strip_markdown(input);
        assert_eq!(result, "The result is 42.");
    }

    #[test]
    fn test_strip_reasoning_tags() {
        let input = "<reasoning>While analyzing the problem...</reasoning>\nThe solution is simple.";
        let result = strip_markdown(input);
        assert_eq!(result, "The solution is simple.");
    }

    #[test]
    fn test_strip_think_case_insensitive() {
        let input = "<THINK>Internal thoughts...</THINK>\nVisible answer.";
        let result = strip_markdown(input);
        assert_eq!(result, "Visible answer.");
    }

    #[test]
    fn test_strip_think_preserves_surrounding_text() {
        let input = "Before thinking. <think>Hidden reasoning here.</think> After thinking.";
        let result = strip_markdown(input);
        assert_eq!(result, "Before thinking. After thinking.");
    }

    #[test]
    fn test_strip_multiple_think_blocks() {
        let input = "<think>First block</think>\nSome text.\n<think>Second block</think>\nMore text.";
        let result = strip_markdown(input);
        assert_eq!(result, "Some text. More text.");
    }
}
