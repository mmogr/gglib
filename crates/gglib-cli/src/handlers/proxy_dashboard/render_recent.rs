//! The recent-requests section of the frame: the last few requests, newest
//! first, each with what gglib did to it in the proxy's own words.
//!
//! Pure, like its siblings: data in, `String` out, no IO and no clock beyond
//! the one the age column is given.

use std::fmt::Write as _;

use super::render::format_elapsed_secs;
use super::wire_recent::RecentRequest;
use crate::presentation::truncate_string;

/// How many of the proxy's recent requests the section shows.
pub(super) const SHOWN: usize = 5;

/// Render the recent-requests section: `recent`, oldest first as the proxy
/// sends it, shown newest first and capped at [`SHOWN`]; a request nothing
/// was done to gets no word.
pub(super) fn render_recent_section(recent: &[RecentRequest]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Recent requests");
    if recent.is_empty() {
        out.push_str("  (none yet)\n");
        return out;
    }
    for request in recent.iter().rev().take(SHOWN) {
        let done: Vec<&str> = request
            .interventions
            .iter()
            .map(|i| i.word.as_str())
            .collect();
        let row = format!(
            "  {:>7} ago  {:<24} {}",
            format_elapsed_secs(request.recorded_at_secs),
            truncate_string(&request.model_name, 24),
            done.join(" \u{b7} ")
        );
        // A row nothing was done to ends after its model, not after padding.
        let _ = writeln!(out, "{}", row.trim_end());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::wire_recent::Intervention;
    use super::*;

    fn request(model: &str, words: &[&str]) -> RecentRequest {
        RecentRequest {
            model_name: model.to_owned(),
            recorded_at_secs: u64::MAX, // the age column is not under test
            interventions: words
                .iter()
                .map(|w| Intervention {
                    word: (*w).to_owned(),
                })
                .collect(),
        }
    }

    /// The words are the proxy's, joined, and a request nothing was done to
    /// ends after its model.
    #[test]
    fn each_row_names_the_model_and_what_was_done_in_the_proxys_words() {
        let out = render_recent_section(&[
            request("qwen3", &[]),
            request("gemma", &["Trimmed 4 old messages", "Noted a loop"]),
        ]);
        let rows: Vec<&str> = out.lines().collect();
        assert_eq!(rows[0], "Recent requests");
        assert!(
            rows[1].contains("gemma")
                && rows[1].ends_with("Trimmed 4 old messages \u{b7} Noted a loop"),
            "{out}"
        );
        assert!(
            rows[2].ends_with("qwen3"),
            "no padding after the model: {out:?}"
        );
    }

    /// Newest first, and no more than the cap.
    #[test]
    fn the_newest_come_first_and_the_section_is_capped() {
        let many: Vec<RecentRequest> = (0..8).map(|i| request(&format!("m{i}"), &[])).collect();
        let out = render_recent_section(&many);
        let models: Vec<String> = out
            .lines()
            .skip(1)
            .map(|l| l.split_whitespace().nth(2).unwrap_or("").to_owned())
            .collect();
        assert_eq!(models, ["m7", "m6", "m5", "m4", "m3"]);
    }

    #[test]
    fn an_empty_ledger_says_so() {
        assert_eq!(
            render_recent_section(&[]),
            "Recent requests\n  (none yet)\n"
        );
    }
}
