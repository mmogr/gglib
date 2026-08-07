//! Chunk-safe scanner for dialect markup that reached client-visible text.
//!
//! Normalization is heuristic: detection can miss a dialect, a template can
//! lie, a model can emit markup its spec does not describe.  When that
//! happens the raw markup flows to the client as ordinary text — silently.
//! [`ResidueScanner`] watches the *post-normalization* client-visible text
//! for known tool-call markers so the proxy can turn that silent breakage
//! into a logged, counted, dashboard-visible signal (the "dialect drift
//! alarm").
//!
//! The scanner never alters the stream — it only observes.  A hit means
//! "a human should look at this model's dialect handling", not "the proxy
//! will fix it".
//!
//! ## Marker set
//!
//! The scan looks for the union of:
//!
//! - the active [`DialectSpec`]'s own markers, when the model has one —
//!   markup surviving *its own* parser is the clearest drift signal; and
//! - [`KNOWN_RESIDUE_MARKERS`], a curated list of tool-call markers from
//!   dialects in the wild.  This deliberately duplicates a handful of
//!   literals from `gglib-gguf`'s detection pattern tables: core cannot
//!   depend on `gglib-gguf`, the list is small and changes rarely, and a
//!   doc cross-reference keeps the two in sight of each other.
//!
//! ## Chunk safety
//!
//! Like the delimited parser, the scanner sees SSE-sized fragments: a
//! marker may straddle chunk boundaries.  [`ResidueScanner::feed`] retains
//! the longest tail of the previous chunk that could be a marker prefix
//! (at most `max marker length − 1` bytes, cut on a char boundary) and
//! prepends it to the next chunk, so a split marker is still found.  The
//! first hit is sticky; scanning short-circuits afterwards.

use crate::domain::dialect::DialectSpec;

/// Tool-call markers from dialects observed in the wild, scanned in
/// addition to the active spec's own markers.
///
/// Curated from `gglib-gguf`'s detection pattern tables
/// (`crates/gglib-gguf/src/capabilities/patterns.rs`) — see the module
/// docs for why this small duplication is deliberate.  Only *tool-call*
/// markers belong here: reasoning tags are handled (and stripped)
/// elsewhere, and generic XML would false-positive on ordinary prose.
pub const KNOWN_RESIDUE_MARKERS: &[&str] = &[
    "<tool_call>",
    "</tool_call>",
    "<function=",
    "[TOOL_CALLS]",
    "<｜tool▁calls▁begin｜>",
    "<｜tool▁call▁begin｜>",
    "<|python_tag|>",
    "functools[",
];

/// Chunk-safe residue scanner.  See the module docs.
#[derive(Debug)]
pub struct ResidueScanner {
    /// Markers to scan for: the active spec's (if any) ∪ the known set.
    markers: Vec<String>,
    /// Longest marker length in bytes — bounds the retained tail.
    max_marker_len: usize,
    /// Tail of the previous chunk that could still open a marker.
    tail: String,
    /// First marker found, if any.  Sticky.
    hit: Option<String>,
}

impl ResidueScanner {
    /// Build a scanner for a model with the given resolved dialect.
    #[must_use]
    pub fn new(dialect: Option<&DialectSpec>) -> Self {
        let mut markers: Vec<String> = KNOWN_RESIDUE_MARKERS
            .iter()
            .map(|m| (*m).to_owned())
            .collect();
        if let Some(spec) = dialect {
            for m in [&spec.tool_open, &spec.tool_close] {
                if !m.is_empty() && !markers.contains(m) {
                    markers.push(m.clone());
                }
            }
        }
        let max_marker_len = markers.iter().map(String::len).max().unwrap_or(0);
        Self {
            markers,
            max_marker_len,
            tail: String::new(),
            hit: None,
        }
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

        if let Some(found) = self.scan(&window) {
            self.hit = Some(found);
            self.tail.clear();
            return;
        }

        // Retain the longest tail that could still be a marker prefix,
        // cut on a char boundary.
        let mut keep = window.len().min(self.max_marker_len.saturating_sub(1));
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

    fn scan(&self, window: &str) -> Option<String> {
        self.markers
            .iter()
            .find(|m| window.contains(m.as_str()))
            .cloned()
    }
}

/// One-shot scan of a complete (non-streaming) text for residue markers.
///
/// Same marker set as [`ResidueScanner`]; chunk safety is trivially
/// satisfied because the whole text is one window.
#[must_use]
pub fn scan_complete(text: &str, dialect: Option<&DialectSpec>) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let scanner = ResidueScanner::new(dialect);
    scanner.scan(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(scanner: &mut ResidueScanner, chunks: &[&str]) {
        for c in chunks {
            scanner.feed(c);
        }
    }

    #[test]
    fn clean_text_never_hits() {
        let mut s = ResidueScanner::new(None);
        feed_all(&mut s, &["hello ", "world, no markup here"]);
        assert_eq!(s.hit(), None);
    }

    #[test]
    fn known_marker_in_one_chunk_hits() {
        let mut s = ResidueScanner::new(None);
        s.feed(r#"text <tool_call>{"name":"x"}</tool_call> more"#);
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn marker_straddling_two_chunks_hits() {
        let mut s = ResidueScanner::new(None);
        feed_all(&mut s, &["before <tool", "_call> after"]);
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn marker_straddling_three_chunks_hits() {
        let mut s = ResidueScanner::new(None);
        feed_all(&mut s, &["x<to", "ol_c", "all>y"]);
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn multibyte_marker_straddling_chunks_hits() {
        // The deepseek markers are multibyte; split mid-scalar-boundary.
        let mut s = ResidueScanner::new(None);
        feed_all(&mut s, &["a<｜tool▁calls", "▁begin｜>b"]);
        assert_eq!(s.hit(), Some("<｜tool▁calls▁begin｜>"));
    }

    #[test]
    fn lookalike_prefix_that_never_completes_does_not_hit() {
        let mut s = ResidueScanner::new(None);
        feed_all(&mut s, &["before <tool", "s> after"]);
        assert_eq!(s.hit(), None);
    }

    #[test]
    fn spec_markers_are_included() {
        let spec = DialectSpec {
            tool_open: "«TC»".to_owned(),
            tool_close: "«/TC»".to_owned(),
            ..DialectSpec::qwen_xml()
        };
        let mut s = ResidueScanner::new(Some(&spec));
        feed_all(&mut s, &["oops «T", "C» leaked"]);
        assert_eq!(s.hit(), Some("«TC»"));
    }

    #[test]
    fn hit_is_sticky() {
        let mut s = ResidueScanner::new(None);
        s.feed("<tool_call>");
        s.feed("<function=");
        assert_eq!(s.hit(), Some("<tool_call>"));
    }

    #[test]
    fn scan_complete_matches_the_streaming_scanner() {
        assert_eq!(
            scan_complete("x <function=grep> y", None),
            Some("<function=".to_owned())
        );
        assert_eq!(scan_complete("clean", None), None);
        assert_eq!(scan_complete("", None), None);
    }
}
