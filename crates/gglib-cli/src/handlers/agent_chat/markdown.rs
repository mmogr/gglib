//! Markdown normalisation and rendering for Rich mode.
//!
//! [`normalize_markdown`] pre-processes LLM-emitted Markdown so that
//! [`termimad`] renders it correctly.  [`render_markdown`] is the public
//! entry point called by the drain loop when a complete response is ready.

use crate::presentation::style;

/// Normalize LLM-emitted Markdown before passing it to termimad.
///
/// 1. Convert `*`-based unordered list markers to `-` so that termimad's
///    minimad parser does not confuse them with bold/italic emphasis
///    (e.g. `* **Bold:** text` → `- **Bold:** text`).
/// 2. Collapse runs of 3+ consecutive blank lines down to one blank line
///    to reduce excessive vertical whitespace.
pub(super) fn normalize_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut consecutive_blank = 0u32;

    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            consecutive_blank += 1;
            // Collapse 3+ consecutive blank lines → 2 (one blank line).
            if consecutive_blank <= 1 {
                if i > 0 {
                    out.push('\n');
                }
                out.push('\n');
            }
            continue;
        }

        // Non-empty line: emit the newline separator if needed.
        if i > 0 && consecutive_blank == 0 {
            out.push('\n');
        }
        consecutive_blank = 0;

        // Replace `*`-based list markers with `-`.
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("* ") {
            let indent = &line[..line.len() - trimmed.len()];
            out.push_str(indent);
            out.push_str("- ");
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Render a Markdown string to stdout through [`termimad`].
pub(super) fn render_markdown(text: &str) {
    let normalized = normalize_markdown(text);
    let skin = style::get_markdown_skin();
    print!("{}", skin.term_text(&normalized));
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::normalize_markdown;

    #[test]
    fn normalize_converts_star_bullets_to_dash() {
        let input = "* first\n* second\n";
        assert_eq!(normalize_markdown(input), "- first\n- second");
    }

    #[test]
    fn normalize_preserves_indented_star_bullets() {
        let input = "  * nested\n    * deep\n";
        assert_eq!(normalize_markdown(input), "  - nested\n    - deep");
    }

    #[test]
    fn normalize_preserves_dash_bullets() {
        let input = "- already dash\n  - nested\n";
        assert_eq!(normalize_markdown(input), "- already dash\n  - nested");
    }

    #[test]
    fn normalize_does_not_touch_bold_stars() {
        let input = "Some **bold** text\n";
        assert_eq!(normalize_markdown(input), "Some **bold** text");
    }

    #[test]
    fn normalize_collapses_excess_blank_lines() {
        let input = "para one\n\n\n\npara two\n";
        assert_eq!(normalize_markdown(input), "para one\n\npara two");
    }

    #[test]
    fn normalize_preserves_single_blank_line() {
        let input = "para one\n\npara two\n";
        assert_eq!(normalize_markdown(input), "para one\n\npara two");
    }

    #[test]
    fn normalize_star_bullet_with_bold() {
        // The exact pattern that confuses termimad's parser.
        let input = "* **April 1-2:** Airstrikes hit\n* **April 3:** Ceasefire talks\n";
        assert_eq!(
            normalize_markdown(input),
            "- **April 1-2:** Airstrikes hit\n- **April 3:** Ceasefire talks"
        );
    }
}
