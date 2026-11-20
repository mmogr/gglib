//! Chunk-safe scanner for dialect markup that reached client-visible text.
//!
//! llama-server parses the tool calls of the templates it knows. When it
//! does not, the raw markup flows to the client as ordinary text — silently.
//! [`ResidueScanner`] watches the client-visible text for known tool-call
//! markers so the proxy can turn that silent breakage into a logged,
//! counted, dashboard-visible signal (the "dialect drift alarm").
//!
//! The scanner never alters the stream — it only observes.  A hit means
//! "a human should look at this model's tool calls", not "the proxy will
//! fix it".
//!
//! ## Marker set
//!
//! [`KNOWN_RESIDUE_MARKERS`], a curated list of tool-call markers from
//! dialects in the wild: the forms models have been seen to write a call in
//! when nothing parsed it.
//!
//! ## A reply that is no answer
//!
//! [`no_answer`] judges a run's final reply by the same markers: a reply
//! with no words, or one that starts with a tool call written as text on a
//! turn that offered no tools, is not an answer. The agent loop and its
//! step-limit closing request both ask it.
//!
//! ## Chunk safety
//!
//! The scanner sees SSE-sized fragments: a
//! marker may straddle chunk boundaries.  [`ResidueScanner::feed`] retains
//! the longest tail of the previous chunk that could be a marker prefix
//! (at most `max marker length − 1` bytes, cut on a char boundary) and
//! prepends it to the next chunk, so a split marker is still found.  The
//! first hit is sticky; scanning short-circuits afterwards.

/// Tool-call markers from dialects observed in the wild.
///
/// Only *tool-call* markers belong here: generic XML would false-positive
/// on ordinary prose.
pub const KNOWN_RESIDUE_MARKERS: &[&str] = &[
    "<tool_call>",
    "</tool_call>",
    "<function=",
    "[TOOL_CALLS]",
    "[TOOL_CALL]",
    "<｜tool▁calls▁begin｜>",
    "<｜tool▁call▁begin｜>",
    "<|python_tag|>",
    "functools[",
];

/// Longest marker length in bytes — bounds the retained tail.
const MAX_MARKER_LEN: usize = {
    let mut longest = 0;
    let mut i = 0;
    while i < KNOWN_RESIDUE_MARKERS.len() {
        if KNOWN_RESIDUE_MARKERS[i].len() > longest {
            longest = KNOWN_RESIDUE_MARKERS[i].len();
        }
        i += 1;
    }
    longest
};

/// Chunk-safe residue scanner.  See the module docs.
#[derive(Debug, Default)]
pub struct ResidueScanner {
    /// Tail of the previous chunk that could still open a marker.
    tail: String,
    /// First marker found, if any.  Sticky.
    hit: Option<String>,
}

impl ResidueScanner {
    /// A scanner for [`KNOWN_RESIDUE_MARKERS`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Scan one chunk of client-visible text.
    ///
    /// Cheap after the first hit (immediately returns).  Never alters the
    /// text — the caller forwards it regardless.
    pub fn feed(&mut self, chunk: &str) {
        if self.hit.is_some() || chunk.is_empty() {
            return;
        }

        // Prepend the held-back tail so straddled markers are visible.
        let window = if self.tail.is_empty() {
            chunk.to_owned()
        } else {
            let mut w = std::mem::take(&mut self.tail);
            w.push_str(chunk);
            w
        };

        if let Some(found) = scan(&window) {
            self.hit = Some(found);
            self.tail.clear();
            return;
        }

        // Retain the longest tail that could still be a marker prefix,
        // cut on a char boundary.
        let mut keep = window.len().min(MAX_MARKER_LEN.saturating_sub(1));
        while keep > 0 && !window.is_char_boundary(window.len() - keep) {
            keep -= 1;
        }
        window[window.len() - keep..].clone_into(&mut self.tail);
    }

    /// The first marker seen in the fed text, if any.
    #[must_use]
    pub fn hit(&self) -> Option<&str> {
        self.hit.as_deref()
    }
}

/// The first known marker in `window`, if any.
fn scan(window: &str) -> Option<String> {
    KNOWN_RESIDUE_MARKERS
        .iter()
        .find(|m| window.contains(*m))
        .map(|m| (*m).to_owned())
}

/// One-shot scan of a complete (non-streaming) text for residue markers.
///
/// Same marker set as [`ResidueScanner`]; chunk safety is trivially
/// satisfied because the whole text is one window.
#[must_use]
pub fn scan_complete(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    scan(text)
}

/// Why a run's final reply is no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoAnswer {
    /// It has no words: it is empty, or only whitespace.
    Wordless,
    /// It starts with this tool-call marker, on a turn that offered no
    /// tools: a call written as text, which is what a model that tries a
    /// call it was not offered writes.
    ToolCall(&'static str),
}

/// Why `text`, a final reply that called no tool, is no answer; `None` when
/// it is one. `tools_offered` is whether the request it answers offered any.
///
/// A reply with no words is never an answer. One that starts with a known
/// tool-call marker ([`KNOWN_RESIDUE_MARKERS`]), leading whitespace aside,
/// is none when no tools were offered, whatever follows the call. Only how
/// the text starts is judged: words that name a marker, or that come before
/// a call, are an answer.
#[must_use]
pub fn no_answer(text: &str, tools_offered: bool) -> Option<NoAnswer> {
    let text = text.trim_start();
    if text.is_empty() {
        return Some(NoAnswer::Wordless);
    }
    if tools_offered {
        return None;
    }
    KNOWN_RESIDUE_MARKERS
        .iter()
        .copied()
        .find(|marker| text.starts_with(marker))
        .map(NoAnswer::ToolCall)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_with_no_words_is_no_answer_whether_or_not_tools_were_offered() {
        for text in ["", " \n\t"] {
            for tools_offered in [true, false] {
                assert_eq!(
                    no_answer(text, tools_offered),
                    Some(NoAnswer::Wordless),
                    "{text:?}"
                );
            }
        }
    }

    #[test]
    fn a_reply_that_starts_with_a_marker_is_no_answer_only_when_no_tools_were_offered() {
        let replies = [
            (
                "<tool_call>\n{\"name\": \"search\"}\n</tool_call>",
                "<tool_call>",
            ),
            (
                "[TOOL_CALL]\n{tool => \"search\"}\n[/TOOL_CALL]",
                "[TOOL_CALL]",
            ),
            ("[TOOL_CALLS][{\"name\": \"search\"}]", "[TOOL_CALLS]"),
            ("\n <tool_call>{}</tool_call>\nI will look.", "<tool_call>"),
        ];
        for (text, marker) in replies {
            assert_eq!(
                no_answer(text, false),
                Some(NoAnswer::ToolCall(marker)),
                "{text:?}"
            );
            assert_eq!(no_answer(text, true), None, "{text:?}");
        }
    }

    #[test]
    fn a_reply_whose_words_come_first_is_an_answer() {
        for text in [
            "The parser reads `<tool_call>` blocks in src/parse.rs.",
            "It is in src/lib.rs.\n<tool_call>{\"name\": \"search\"}</tool_call>",
        ] {
            for tools_offered in [true, false] {
                assert_eq!(no_answer(text, tools_offered), None, "{text:?}");
            }
        }
    }

    fn feed_all(scanner: &mut ResidueScanner, chunks: &[&str]) {
        for c in chunks {
            scanner.feed(c);
        }
    }

    #[test]
    fn clean_text_never_hits() {
        let mut s = ResidueScanner::new();
        feed_all(&mut s, &["hello ", "world, no markup here"]);
        assert_eq!(s.hit(), None);
    }

    #[test]
    fn known_marker_in_one_chunk_hits() {
        let mut s = ResidueScanner::new();
        s.feed(r#"text <tool_call>{"name":"x"}</tool_call> more"#);
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn marker_straddling_two_chunks_hits() {
        let mut s = ResidueScanner::new();
        feed_all(&mut s, &["before <tool", "_call> after"]);
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn marker_straddling_three_chunks_hits() {
        let mut s = ResidueScanner::new();
        feed_all(&mut s, &["x<to", "ol_c", "all>y"]);
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn multibyte_marker_straddling_chunks_hits() {
        // The deepseek markers are multibyte; split mid-scalar-boundary.
        let mut s = ResidueScanner::new();
        feed_all(&mut s, &["a<｜tool▁calls", "▁begin｜>b"]);
        assert_eq!(s.hit(), Some("<｜tool▁calls▁begin｜>"));
    }

    #[test]
    fn lookalike_prefix_that_never_completes_does_not_hit() {
        let mut s = ResidueScanner::new();
        feed_all(&mut s, &["before <tool", "s> after"]);
        assert_eq!(s.hit(), None);
    }

    #[test]
    fn hit_is_sticky() {
        let mut s = ResidueScanner::new();
        s.feed("<tool_call>");
        s.feed("<function=");
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn scan_complete_matches_the_streaming_scanner() {
        assert_eq!(
            scan_complete("x <function=grep> y"),
            Some("<function=".to_owned())
        );
        assert_eq!(scan_complete("clean"), None);
        assert_eq!(scan_complete(""), None);
    }
}
