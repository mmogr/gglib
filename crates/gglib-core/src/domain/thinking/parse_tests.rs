//! Tests for the parsing, embedding, detection, and formatting functions.

use super::parse::{
    embed_thinking_content, format_thinking_duration, has_thinking_content, parse_thinking_content,
};

// ---------------------------------------------------------------------------
// parse_thinking_content
// ---------------------------------------------------------------------------

mod parse {
    use super::*;

    #[test]
    fn empty_input() {
        let r = parse_thinking_content("");
        assert_eq!(r.thinking, None);
        assert_eq!(r.content, "");
        assert_eq!(r.duration_seconds, None);
    }

    #[test]
    fn no_thinking_tags() {
        let input = "Just a normal response without thinking";
        let r = parse_thinking_content(input);
        assert_eq!(r.thinking, None);
        assert_eq!(r.content, input);
    }

    #[test]
    fn standard_think_tags() {
        let input = "<think>I need to analyze this carefully</think>\nHere is my response.";
        let r = parse_thinking_content(input);
        assert_eq!(
            r.thinking.as_deref(),
            Some("I need to analyze this carefully")
        );
        assert_eq!(r.content, "Here is my response.");
        assert!(r.duration_seconds.is_none());
    }

    #[test]
    fn with_duration_attribute() {
        let input = "<think duration=\"5.2\">Quick thinking</think>\nResponse here.";
        let r = parse_thinking_content(input);
        assert_eq!(r.thinking.as_deref(), Some("Quick thinking"));
        assert_eq!(r.content, "Response here.");
        assert_eq!(r.duration_seconds, Some(5.2));
    }

    #[test]
    fn multiline_thinking() {
        let input = "<think>\nFirst line\nSecond line\nThird line\n</think>\nThe actual response.";
        let r = parse_thinking_content(input);
        let t = r.thinking.unwrap();
        assert!(t.contains("First line"));
        assert!(t.contains("Third line"));
        assert_eq!(r.content, "The actual response.");
    }

    #[test]
    fn reasoning_tags() {
        let input = "<reasoning>Deep analysis here</reasoning>\nFinal answer.";
        let r = parse_thinking_content(input);
        assert_eq!(r.thinking.as_deref(), Some("Deep analysis here"));
        assert_eq!(r.content, "Final answer.");
    }

    #[test]
    fn seed_think_tags() {
        let input = "<seed:think>Seed model thinking</seed:think>\nSeed response.";
        let r = parse_thinking_content(input);
        assert_eq!(r.thinking.as_deref(), Some("Seed model thinking"));
        assert_eq!(r.content, "Seed response.");
    }

    #[test]
    fn command_r_tags() {
        let input = "<|START_THINKING|>Command R analysis<|END_THINKING|>\nCommand R response.";
        let r = parse_thinking_content(input);
        assert_eq!(r.thinking.as_deref(), Some("Command R analysis"));
        assert_eq!(r.content, "Command R response.");
    }

    #[test]
    fn tags_not_at_start_are_ignored() {
        let input = "Some prefix <think>thinking</think> response";
        let r = parse_thinking_content(input);
        assert_eq!(r.thinking, None);
        assert_eq!(r.content, input);
    }

    #[test]
    fn empty_thinking_content() {
        let input = "<think></think>Just the response";
        let r = parse_thinking_content(input);
        assert_eq!(r.thinking, None);
        assert_eq!(r.content, "Just the response");
    }
}

// ---------------------------------------------------------------------------
// embed_thinking_content
// ---------------------------------------------------------------------------

mod embed {
    use super::*;

    #[test]
    fn no_thinking_returns_content() {
        assert_eq!(embed_thinking_content(None, "Response", None), "Response");
        assert_eq!(
            embed_thinking_content(Some(""), "Response", None),
            "Response"
        );
    }

    #[test]
    fn with_thinking() {
        let r = embed_thinking_content(Some("My thinking"), "My response", None);
        assert_eq!(r, "<think>My thinking</think>\nMy response");
    }

    #[test]
    fn with_duration() {
        let r = embed_thinking_content(Some("My thinking"), "My response", Some(5.2));
        assert_eq!(
            r,
            "<think duration=\"5.2\">My thinking</think>\nMy response"
        );
    }

    #[test]
    fn zero_duration() {
        let r = embed_thinking_content(Some("Quick"), "Response", Some(0.0));
        assert_eq!(r, "<think duration=\"0.0\">Quick</think>\nResponse");
    }

    #[test]
    fn roundtrip_with_parse() {
        let thinking = "Original thinking";
        let content = "Original response";
        let duration = 3.5;

        let embedded = embed_thinking_content(Some(thinking), content, Some(duration));
        let parsed = parse_thinking_content(&embedded);

        assert_eq!(parsed.thinking.as_deref(), Some(thinking));
        assert_eq!(parsed.content, content);
        assert_eq!(parsed.duration_seconds, Some(duration));
    }
}

// ---------------------------------------------------------------------------
// has_thinking_content
// ---------------------------------------------------------------------------

mod has_thinking {
    use super::*;

    #[test]
    fn empty_returns_false() {
        assert!(!has_thinking_content(""));
    }

    #[test]
    fn think_tag() {
        assert!(has_thinking_content("<think>content</think>"));
        assert!(has_thinking_content("  <think>with whitespace"));
    }

    #[test]
    fn reasoning_tag() {
        assert!(has_thinking_content("<reasoning>content</reasoning>"));
        assert!(has_thinking_content("\n<reasoning>newline prefix"));
    }

    #[test]
    fn seed_think_tag() {
        assert!(has_thinking_content("<seed:think>content</seed:think>"));
    }

    #[test]
    fn command_r_tag() {
        assert!(has_thinking_content("<|START_THINKING|>content"));
        assert!(has_thinking_content("<|start_thinking|>lowercase"));
    }

    #[test]
    fn not_at_start_returns_false() {
        assert!(!has_thinking_content("prefix <think>content</think>"));
        assert!(!has_thinking_content(
            "Hello <reasoning>world</reasoning>"
        ));
    }

    #[test]
    fn case_insensitive() {
        assert!(has_thinking_content("<THINK>upper"));
        assert!(has_thinking_content("<Reasoning>mixed"));
    }
}

// ---------------------------------------------------------------------------
// format_thinking_duration
// ---------------------------------------------------------------------------

mod format_duration {
    use super::*;

    #[test]
    fn under_sixty_seconds() {
        assert_eq!(format_thinking_duration(0.0), "0.0s");
        assert_eq!(format_thinking_duration(1.0), "1.0s");
        assert_eq!(format_thinking_duration(5.5), "5.5s");
        assert_eq!(format_thinking_duration(59.9), "59.9s");
    }

    #[test]
    fn minutes_and_seconds() {
        assert_eq!(format_thinking_duration(60.0), "1m 0s");
        assert_eq!(format_thinking_duration(61.0), "1m 1s");
        assert_eq!(format_thinking_duration(90.0), "1m 30s");
        assert_eq!(format_thinking_duration(125.0), "2m 5s");
    }

    #[test]
    fn large_values() {
        assert_eq!(format_thinking_duration(3600.0), "60m 0s");
        assert_eq!(format_thinking_duration(3665.0), "61m 5s");
    }
}
