//! Tests for [`super::accumulator::ThinkingAccumulator`].

use super::accumulator::ThinkingAccumulator;
use super::types::ThinkingEvent;

// ---------------------------------------------------------------------------
// Basic scenarios
// ---------------------------------------------------------------------------

mod basic {
    use super::*;

    #[test]
    fn no_tags_everything_is_content() {
        let mut acc = ThinkingAccumulator::new();
        let events = acc.push("Hello world");
        assert_eq!(
            events,
            vec![ThinkingEvent::ContentDelta("Hello world".into())]
        );
    }

    #[test]
    fn complete_think_block_in_one_chunk() {
        let mut acc = ThinkingAccumulator::new();
        let events = acc.push("<think>hmm</think>Answer");
        assert_eq!(
            events,
            vec![
                ThinkingEvent::ThinkingDelta("hmm".into()),
                ThinkingEvent::ThinkingEnd,
                ThinkingEvent::ContentDelta("Answer".into()),
            ]
        );
    }

    #[test]
    fn think_tag_then_content_separately() {
        let mut acc = ThinkingAccumulator::new();

        let e1 = acc.push("<think>");
        assert_eq!(e1, vec![]);

        let e2 = acc.push("reasoning here");
        assert_eq!(
            e2,
            vec![ThinkingEvent::ThinkingDelta("reasoning here".into())]
        );

        let e3 = acc.push("</think>");
        assert_eq!(e3, vec![ThinkingEvent::ThinkingEnd]);

        let e4 = acc.push("Final answer");
        assert_eq!(e4, vec![ThinkingEvent::ContentDelta("Final answer".into())]);
    }

    #[test]
    fn empty_chunks_produce_nothing() {
        let mut acc = ThinkingAccumulator::new();
        assert!(acc.push("").is_empty());
    }

    #[test]
    fn content_after_end_stays_content() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("<think>x</think>");
        let e = acc.push("more");
        assert_eq!(e, vec![ThinkingEvent::ContentDelta("more".into())]);
        let e2 = acc.push(" stuff");
        assert_eq!(e2, vec![ThinkingEvent::ContentDelta(" stuff".into())]);
    }
}

// ---------------------------------------------------------------------------
// Split-tag edge cases
// ---------------------------------------------------------------------------

mod split_tags {
    use super::*;

    #[test]
    fn open_tag_split_across_two_chunks() {
        let mut acc = ThinkingAccumulator::new();

        let e1 = acc.push("<thi");
        assert!(e1.is_empty(), "partial open tag should buffer");

        let e2 = acc.push("nk>");
        assert_eq!(e2, vec![]); // open tag consumed

        let e3 = acc.push("thought");
        assert_eq!(e3, vec![ThinkingEvent::ThinkingDelta("thought".into())]);
    }

    #[test]
    fn open_tag_split_across_three_chunks() {
        let mut acc = ThinkingAccumulator::new();
        assert!(acc.push("<").is_empty());
        assert!(acc.push("thin").is_empty());
        assert_eq!(acc.push("k>"), vec![]);

        let e = acc.push("data");
        assert_eq!(e, vec![ThinkingEvent::ThinkingDelta("data".into())]);
    }

    #[test]
    fn close_tag_split_across_two_chunks() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("<think>");

        let e1 = acc.push("thinking</thi");
        // "thinking" should be emitted, "</thi" buffered
        assert_eq!(e1, vec![ThinkingEvent::ThinkingDelta("thinking".into())]);

        let e2 = acc.push("nk>");
        assert_eq!(e2, vec![ThinkingEvent::ThinkingEnd]);
    }

    #[test]
    fn close_tag_split_across_three_chunks() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("<think>");

        let e1 = acc.push("data</");
        assert_eq!(e1, vec![ThinkingEvent::ThinkingDelta("data".into())]);

        let e2 = acc.push("thin");
        assert!(e2.is_empty(), "partial close tag should buffer");

        let e3 = acc.push("k>");
        assert_eq!(e3, vec![ThinkingEvent::ThinkingEnd]);
    }

    #[test]
    fn close_tag_split_byte_by_byte() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("<think>x");

        // Feed close tag char by char
        let chars = "</think>";
        let mut all_events = vec![];

        // First, the 'x' should have been emitted as ThinkingDelta.
        // Let's check the full sequence.
        for c in chars.chars() {
            let events = acc.push(&c.to_string());
            all_events.extend(events);
        }

        assert!(
            all_events.contains(&ThinkingEvent::ThinkingEnd),
            "ThinkingEnd must appear: {all_events:?}"
        );
    }

    #[test]
    fn open_tag_with_attributes_split() {
        let mut acc = ThinkingAccumulator::new();
        assert!(acc.push("<think dur").is_empty());
        assert!(acc.push("ation=\"3.5\"").is_empty());
        let e = acc.push(">content");
        assert_eq!(e, vec![ThinkingEvent::ThinkingDelta("content".into())]);
    }

    #[test]
    fn false_alarm_lt_inside_thinking() {
        // A '<' inside thinking that is NOT a close tag.
        let mut acc = ThinkingAccumulator::new();
        acc.push("<think>");

        let e = acc.push("a < b");
        // The '<' doesn't start "</think>" so should be emitted.
        // But it might be buffered initially. Let's check combined output.
        let mut all = e;
        all.extend(acc.flush());

        let text: String = all
            .iter()
            .filter_map(|e| match e {
                ThinkingEvent::ThinkingDelta(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            text.contains("a < b"),
            "false alarm '<' should pass through: {text}"
        );
    }

    #[test]
    fn normalized_tag_split() {
        let mut acc = ThinkingAccumulator::new();

        let e1 = acc.push("<reasoning>");
        // <reasoning> is normalized to <think> in a single chunk
        assert_eq!(e1, vec![]);

        let e2 = acc.push("content");
        assert_eq!(e2, vec![ThinkingEvent::ThinkingDelta("content".into())]);

        let e3 = acc.push("</reasoning>");
        assert_eq!(e3, vec![ThinkingEvent::ThinkingEnd]);
    }
}

// ---------------------------------------------------------------------------
// Flush behaviour
// ---------------------------------------------------------------------------

mod flush {
    use super::*;

    #[test]
    fn flush_partial_open_tag_as_content() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("<thi");
        let events = acc.flush();
        assert_eq!(events, vec![ThinkingEvent::ContentDelta("<thi".into())]);
    }

    #[test]
    fn flush_inside_thinking_emits_remaining() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("<think>");
        // "partial</thi" — the "</thi" suffix is a potential close-tag prefix
        // so it stays buffered; flush should emit it as ThinkingDelta.
        acc.push("partial</thi");
        let events = acc.flush();
        let has_thinking = events
            .iter()
            .any(|e| matches!(e, ThinkingEvent::ThinkingDelta(_)));
        assert!(has_thinking, "should emit remaining thinking on flush");
    }

    #[test]
    fn flush_when_empty_returns_nothing() {
        let mut acc = ThinkingAccumulator::new();
        assert!(acc.flush().is_empty());
    }

    #[test]
    fn flush_in_content_phase_emits_nothing() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("no tags here");
        // After non-tag text, state is ContentPhase and buffer is clear.
        let events = acc.flush();
        assert!(events.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Realistic scenarios
// ---------------------------------------------------------------------------

mod realistic {
    use super::*;

    /// Simulate a realistic stream: open tag, multiple thinking deltas,
    /// close tag, then content.
    #[test]
    fn typical_reasoning_model_stream() {
        let mut acc = ThinkingAccumulator::new();
        let chunks = vec![
            "<think>",
            "Let me think about this.\n",
            "First, I should consider...\n",
            "Actually, the answer is simple.\n",
            "</think>",
            "\n",
            "The answer is 42.",
        ];

        let mut thinking = String::new();
        let mut content = String::new();
        let mut saw_end = false;

        for chunk in chunks {
            for event in acc.push(chunk) {
                match event {
                    ThinkingEvent::ThinkingDelta(t) => thinking.push_str(&t),
                    ThinkingEvent::ThinkingEnd => saw_end = true,
                    ThinkingEvent::ContentDelta(c) => content.push_str(&c),
                }
            }
        }
        for event in acc.flush() {
            match event {
                ThinkingEvent::ThinkingDelta(t) => thinking.push_str(&t),
                ThinkingEvent::ThinkingEnd => saw_end = true,
                ThinkingEvent::ContentDelta(c) => content.push_str(&c),
            }
        }

        assert!(thinking.contains("Let me think"));
        assert!(thinking.contains("answer is simple"));
        assert!(saw_end);
        assert!(content.contains("The answer is 42"));
    }

    /// No thinking at all — all content.
    #[test]
    fn no_thinking_stream() {
        let mut acc = ThinkingAccumulator::new();
        let chunks = vec!["Hello, ", "how are ", "you?"];

        let mut content = String::new();
        for chunk in chunks {
            for event in acc.push(chunk) {
                match event {
                    ThinkingEvent::ContentDelta(c) => content.push_str(&c),
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        }

        assert_eq!(content, "Hello, how are you?");
    }

    /// Close tag arrives with content fused: "...</think>The answer"
    #[test]
    fn close_tag_fused_with_content() {
        let mut acc = ThinkingAccumulator::new();
        acc.push("<think>");

        let events = acc.push("hmm</think>The answer");
        let mut thinking = String::new();
        let mut content = String::new();
        let mut saw_end = false;

        for e in events {
            match e {
                ThinkingEvent::ThinkingDelta(t) => thinking.push_str(&t),
                ThinkingEvent::ThinkingEnd => saw_end = true,
                ThinkingEvent::ContentDelta(c) => content.push_str(&c),
            }
        }

        assert_eq!(thinking, "hmm");
        assert!(saw_end);
        assert_eq!(content, "The answer");
    }
}
