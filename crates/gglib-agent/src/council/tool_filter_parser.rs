//! Parser for the council agent tool-filter input syntax.
//!
//! This module provides [`parse_tool_filter`], the canonical implementation used
//! by every frontend (CLI, Axum web API, Tauri GUI) so that tool-filter entry
//! behaviour is **identical** across all surfaces.
//!
//! # Syntax
//!
//! Input is a comma-separated list of *tokens*.  Whitespace around commas and
//! around each token is ignored.  The following token forms are accepted:
//!
//! | Token | Meaning |
//! |-------|---------|
//! | `all` | All available tools (case-insensitive) |
//! | `5` | The tool at 1-based position 5 in the available list |
//! | `5:9` | Tools at positions 5 through 9 inclusive (1-based) |
//! | `name` | The tool whose name is exactly `name` |
//! | `!5` | *Exclude* tool at position 5 |
//! | `!5:9` | *Exclude* tools at positions 5–9 |
//! | `!name` | *Exclude* the named tool |
//!
//! ## Exclusion semantics
//!
//! When the input contains **only exclusion tokens** (e.g. `!6`), the
//! inclusion set is implicitly the full available list — "all tools except
//! those excluded".
//!
//! ## Return value
//!
//! - [`None`] — the agent should receive all available tools (equivalent to
//!   clearing the filter).
//! - [`Some(names)`] — an explicit allowlist of tool names.
//!
//! # Examples
//!
//! ```
//! # use gglib_agent::council::parse_tool_filter;
//! let tools: Vec<String> = (1..=9)
//!     .map(|i| format!("tool_{i}"))
//!     .collect();
//!
//! // Numeric range
//! let r = parse_tool_filter("5:7", &tools).unwrap();
//! assert_eq!(r, Some(vec!["tool_5".into(), "tool_6".into(), "tool_7".into()]));
//!
//! // Exclusion only — implicit "all except"
//! let r = parse_tool_filter("!1", &tools).unwrap();
//! assert_eq!(r.unwrap().len(), 8);
//!
//! // Mixed range + exclusion
//! let r = parse_tool_filter("5:9,!6", &tools).unwrap();
//! assert_eq!(r, Some(vec!["tool_5".into(), "tool_7".into(), "tool_8".into(), "tool_9".into()]));
//!
//! // "all" or empty → None
//! assert_eq!(parse_tool_filter("all", &tools).unwrap(), None);
//! assert_eq!(parse_tool_filter("", &tools).unwrap(), None);
//! ```

use anyhow::{Result, bail};

// ─── Public API ─────────────────────────────────────────────────────────────

/// Parse a tool-filter expression and return the resolved allowlist.
///
/// `available` is the ordered list of tool names the executor exposes.  The
/// caller is responsible for printing these to the user (with 1-based indices)
/// before collecting input.
///
/// Returns `Ok(None)` when all tools should be allowed; `Ok(Some(names))` for
/// an explicit subset; `Err` on invalid syntax or out-of-range indices.
pub fn parse_tool_filter(input: &str, available: &[String]) -> Result<Option<Vec<String>>> {
    let trimmed = input.trim();

    // Fast path: empty or literal "all"
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
        return Ok(None);
    }

    // Tokenise on commas
    let tokens: Vec<&str> = trimmed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let mut inclusions: Vec<String> = Vec::new();
    let mut exclusions: Vec<String> = Vec::new();
    let mut has_explicit_inclusions = false;

    for token in &tokens {
        if let Some(rest) = token.strip_prefix('!') {
            let resolved = resolve_token(rest.trim(), available)?;
            exclusions.extend(resolved);
        } else {
            has_explicit_inclusions = true;
            let resolved = resolve_token(token, available)?;
            inclusions.extend(resolved);
        }
    }

    // If only exclusions were given, start from the full available set
    let mut selected: Vec<String> = if has_explicit_inclusions {
        inclusions
    } else {
        available.to_vec()
    };

    // Deduplicate (preserving first-occurrence order)
    dedup_preserve_order(&mut selected);

    // Remove exclusions
    selected.retain(|name| !exclusions.contains(name));

    // If the result equals the full available set, return None (no filter needed)
    if selected == available {
        return Ok(None);
    }

    Ok(Some(selected))
}

// ─── Private helpers ─────────────────────────────────────────────────────────

/// Resolve a single (non-`!`-prefixed) token into a list of tool names.
///
/// Handles:
/// - `"all"` → all available names
/// - `"N"` (integer) → the tool at 1-based index N
/// - `"N:M"` (range, both sides numeric) → tools at positions N through M inclusive
/// - anything else → exact name lookup
fn resolve_token(token: &str, available: &[String]) -> Result<Vec<String>> {
    if token.eq_ignore_ascii_case("all") {
        return Ok(available.to_vec());
    }

    // Range: N:M — only when both sides are purely numeric so that tool names
    // containing ':' (e.g. "tavily:search") are not mis-parsed as ranges.
    if let Some((start_str, end_str)) = token.split_once(':') {
        let start_str = start_str.trim();
        let end_str = end_str.trim();
        if start_str.bytes().all(|b| b.is_ascii_digit())
            && end_str.bytes().all(|b| b.is_ascii_digit())
        {
            let start = parse_index(start_str, available.len())?;
            let end = parse_index(end_str, available.len())?;
            if start > end {
                bail!(
                    "range start ({}) must not be greater than range end ({})",
                    start + 1,
                    end + 1
                );
            }
            return Ok(available[start..=end].to_vec());
        }
    }

    // Single integer index
    if token.bytes().all(|b| b.is_ascii_digit()) {
        let idx = parse_index(token, available.len())?;
        return Ok(vec![available[idx].clone()]);
    }

    // Exact name
    if available.iter().any(|a| a == token) {
        return Ok(vec![token.to_owned()]);
    }

    bail!("unknown tool: {token}")
}

/// Parse a 1-based index string into a 0-based `usize`, validating bounds.
fn parse_index(s: &str, len: usize) -> Result<usize> {
    let n: usize = s
        .parse()
        .map_err(|_| anyhow::anyhow!("expected a number, got {s:?}"))?;
    if n == 0 || n > len {
        bail!("tool index {n} is out of range (1–{len})");
    }
    Ok(n - 1)
}

/// Remove duplicates from `v`, keeping the first occurrence of each element.
fn dedup_preserve_order(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|item| seen.insert(item.clone()));
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("tool_{i}")).collect()
    }

    fn named_tools() -> Vec<String> {
        vec![
            "builtin:get_current_time".into(),
            "builtin:read_file".into(),
            "builtin:list_directory".into(),
            "builtin:grep_search".into(),
            "tavily:search".into(),
            "tavily:extract".into(),
            "tavily:crawl".into(),
            "tavily:map".into(),
            "tavily:research".into(),
        ]
    }

    // ── fast-path ────────────────────────────────────────────────────────────

    #[test]
    fn empty_input_returns_none() {
        assert_eq!(parse_tool_filter("", &tools(5)).unwrap(), None);
    }

    #[test]
    fn whitespace_only_returns_none() {
        assert_eq!(parse_tool_filter("   ", &tools(5)).unwrap(), None);
    }

    #[test]
    fn all_keyword_returns_none() {
        assert_eq!(parse_tool_filter("all", &tools(5)).unwrap(), None);
    }

    #[test]
    fn all_keyword_case_insensitive() {
        assert_eq!(parse_tool_filter("ALL", &tools(5)).unwrap(), None);
        assert_eq!(parse_tool_filter("All", &tools(5)).unwrap(), None);
    }

    // ── numeric single ───────────────────────────────────────────────────────

    #[test]
    fn numeric_single_index() {
        let t = tools(9);
        assert_eq!(
            parse_tool_filter("5", &t).unwrap(),
            Some(vec!["tool_5".into()])
        );
    }

    #[test]
    fn numeric_index_1() {
        let t = tools(9);
        assert_eq!(
            parse_tool_filter("1", &t).unwrap(),
            Some(vec!["tool_1".into()])
        );
    }

    #[test]
    fn numeric_index_last() {
        let t = tools(9);
        assert_eq!(
            parse_tool_filter("9", &t).unwrap(),
            Some(vec!["tool_9".into()])
        );
    }

    // ── range ────────────────────────────────────────────────────────────────

    #[test]
    fn range_5_to_9() {
        let t = tools(9);
        let got = parse_tool_filter("5:9", &t).unwrap();
        assert_eq!(
            got,
            Some(vec![
                "tool_5".into(),
                "tool_6".into(),
                "tool_7".into(),
                "tool_8".into(),
                "tool_9".into(),
            ])
        );
    }

    #[test]
    fn range_single_element() {
        let t = tools(9);
        assert_eq!(
            parse_tool_filter("3:3", &t).unwrap(),
            Some(vec!["tool_3".into()])
        );
    }

    // ── exclusion only ───────────────────────────────────────────────────────

    #[test]
    fn exclusion_only_single() {
        let t = tools(9);
        let got = parse_tool_filter("!6", &t).unwrap().unwrap();
        assert_eq!(got.len(), 8);
        assert!(!got.contains(&"tool_6".to_string()));
    }

    #[test]
    fn exclusion_only_range() {
        let t = tools(9);
        let got = parse_tool_filter("!5:9", &t).unwrap().unwrap();
        assert_eq!(got, vec!["tool_1", "tool_2", "tool_3", "tool_4"]);
    }

    #[test]
    fn exclusion_only_by_name() {
        let t = named_tools();
        let got = parse_tool_filter("!tavily:search", &t).unwrap().unwrap();
        assert!(!got.contains(&"tavily:search".to_string()));
        assert_eq!(got.len(), 8);
    }

    // ── mixed inclusion + exclusion ──────────────────────────────────────────

    #[test]
    fn range_with_exclusion() {
        let t = tools(9);
        // 5:9 minus 6 → [5, 7, 8, 9]
        let got = parse_tool_filter("5:9,!6", &t).unwrap();
        assert_eq!(
            got,
            Some(vec![
                "tool_5".into(),
                "tool_7".into(),
                "tool_8".into(),
                "tool_9".into(),
            ])
        );
    }

    #[test]
    fn range_with_exclusion_range() {
        let t = tools(9);
        // 1:9 minus !5:9 → [1, 2, 3, 4]
        let got = parse_tool_filter("1:9,!5:9", &t).unwrap();
        assert_eq!(
            got,
            Some(vec![
                "tool_1".into(),
                "tool_2".into(),
                "tool_3".into(),
                "tool_4".into(),
            ])
        );
    }

    // ── exact names ─────────────────────────────────────────────────────────

    #[test]
    fn exact_name_works() {
        let t = named_tools();
        let got = parse_tool_filter("tavily:search", &t).unwrap();
        assert_eq!(got, Some(vec!["tavily:search".into()]));
    }

    #[test]
    fn mixed_name_and_number() {
        let t = named_tools();
        // Index 1 = "builtin:get_current_time", plus named "tavily:search" (index 5)
        let got = parse_tool_filter("1,tavily:search", &t).unwrap();
        assert_eq!(
            got,
            Some(vec![
                "builtin:get_current_time".into(),
                "tavily:search".into(),
            ])
        );
    }

    // ── deduplication ────────────────────────────────────────────────────────

    #[test]
    fn duplicates_are_removed() {
        let t = tools(9);
        // "1,1,1:3" → [tool_1, tool_2, tool_3]
        let got = parse_tool_filter("1,1,1:3", &t).unwrap();
        assert_eq!(
            got,
            Some(vec!["tool_1".into(), "tool_2".into(), "tool_3".into()])
        );
    }

    // ── result equals full set → None ────────────────────────────────────────

    #[test]
    fn selecting_all_explicitly_returns_none() {
        let t = tools(3);
        // 1:3 covers everything → equivalent to "all"
        assert_eq!(parse_tool_filter("1:3", &t).unwrap(), None);
    }

    // ── error cases ──────────────────────────────────────────────────────────

    #[test]
    fn out_of_range_index_is_error() {
        let t = tools(5);
        assert!(parse_tool_filter("6", &t).is_err());
    }

    #[test]
    fn index_zero_is_error() {
        let t = tools(5);
        assert!(parse_tool_filter("0", &t).is_err());
    }

    #[test]
    fn unknown_name_is_error() {
        let t = tools(5);
        assert!(parse_tool_filter("nonexistent_tool", &t).is_err());
    }

    #[test]
    fn range_start_gt_end_is_error() {
        let t = tools(9);
        assert!(parse_tool_filter("9:5", &t).is_err());
    }

    #[test]
    fn exclusion_unknown_name_is_error() {
        let t = tools(5);
        assert!(parse_tool_filter("!nonexistent_tool", &t).is_err());
    }
}
