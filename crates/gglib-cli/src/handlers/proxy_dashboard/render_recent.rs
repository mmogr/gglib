//! The recent-requests section of the frame: the last few requests, newest
//! first, each with the tokens it wrote and how fast (the proxy's rate, in
//! [`rate_words`]), and what gglib did to it in the proxy's own words.
//!
//! Pure, like its siblings: data in, `String` out, no IO and no clock beyond
//! the one the age column is given.

use std::fmt::Write as _;

use gglib_core::domain::figures::{count_words, rate_words};

use super::render::format_elapsed_secs;
use super::wire_recent::RecentRequest;
use crate::presentation::truncate_string;

/// How many of the proxy's recent requests the section shows.
pub(super) const SHOWN: usize = 5;

/// What a request wrote and how fast, "812 tokens · 34 t/s", each part only
/// when the proxy has it; empty when it has neither.
fn figures(request: &RecentRequest) -> String {
    [
        request.completion_tokens.map(|tokens| {
            let unit = if tokens == 1 { "token" } else { "tokens" };
            format!("{} {unit}", count_words(u64::from(tokens)))
        }),
        request.tokens_per_second.map(rate_words),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// Render the recent-requests section: `recent`, oldest first as the proxy
/// sends it, shown newest first and capped at [`SHOWN`]; a request with no
/// figures and nothing done to it gets neither.
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
        let said: Vec<String> = [figures(request), done.join(" \u{b7} ")]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
        let row = format!(
            "  {:>7} ago  {:<24} {}",
            format_elapsed_secs(request.recorded_at_secs),
            truncate_string(&request.model_name, 24),
            said.join("  ")
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
            completion_tokens: None,
            tokens_per_second: None,
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

    /// The tokens and the rate are the proxy's, said before what was done;
    /// a request with no time writing has no rate, and one token is a token.
    #[test]
    fn a_row_says_what_the_request_wrote_and_how_fast() {
        let fast = RecentRequest {
            completion_tokens: Some(812),
            tokens_per_second: Some(34.4),
            ..request("qwen3", &["Noted a loop"])
        };
        let one = RecentRequest {
            completion_tokens: Some(1),
            ..request("gemma", &[])
        };
        let slow = RecentRequest {
            completion_tokens: Some(1_200),
            tokens_per_second: Some(8.46),
            ..request("phi", &[])
        };
        let out = render_recent_section(&[slow, one, fast]);
        let rows: Vec<&str> = out.lines().collect();
        assert!(
            rows[1].ends_with("qwen3                    812 tokens \u{b7} 34 t/s  Noted a loop"),
            "{out}"
        );
        assert!(
            rows[2].ends_with("gemma                    1 token"),
            "{out}"
        );
        assert!(
            rows[3].ends_with("phi                      1,200 tokens \u{b7} 8.5 t/s"),
            "{out}"
        );
    }

    #[test]
    fn an_empty_ledger_says_so() {
        assert_eq!(
            render_recent_section(&[]),
            "Recent requests\n  (none yet)\n"
        );
    }
}
