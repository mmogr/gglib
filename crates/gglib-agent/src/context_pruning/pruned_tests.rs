//! The count of what pruning dropped: nothing within budget, each prune's
//! drops added to the last, and never taken back.

use gglib_core::{AgentConfig, AgentMessage};

use super::Pruned;

fn user(content: &str) -> AgentMessage {
    AgentMessage::User {
        content: content.to_owned(),
        images: Vec::new(),
    }
}

/// A budget `chars` wide that keeps the last `tail` messages when it prunes.
fn budget(chars: usize, tail: usize) -> AgentConfig {
    let mut config = AgentConfig::default();
    config.context_budget_chars = chars;
    config.prune_keep_tail_messages = tail;
    config
}

/// `count` user messages of 100 characters each.
fn hundreds(count: usize) -> Vec<AgentMessage> {
    (0..count).map(|_| user(&"x".repeat(100))).collect()
}

#[test]
fn messages_within_budget_drop_nothing() {
    let pruned = Pruned::new(hundreds(3), &budget(10_000, 2));
    assert_eq!(pruned.len(), 3);
    assert_eq!(pruned.dropped(), 0);
}

#[test]
fn a_first_prune_counts_what_it_dropped() {
    let pruned = Pruned::new(hundreds(6), &budget(250, 2));
    assert_eq!(pruned.len(), 2, "the tail is kept");
    assert_eq!(pruned.dropped(), 4);
}

/// The count runs over the whole run: a second prune adds its drops to the
/// first's, and a prune that drops nothing leaves the count where it was.
#[test]
fn each_prune_adds_what_it_dropped() {
    let config = budget(250, 2);
    let mut pruned = Pruned::new(hundreds(6), &config);
    assert_eq!(pruned.dropped(), 4);

    pruned.extend(hundreds(3));
    pruned.prune(&config);
    assert_eq!(pruned.len(), 2);
    assert_eq!(pruned.dropped(), 7, "four, then three more");

    pruned.prune(&config);
    assert_eq!(pruned.dropped(), 7, "nothing more to drop");
}

/// It reads and writes as the messages it holds, so the loop's own helpers
/// take it where they took the `Vec`.
#[test]
fn it_is_read_and_written_as_its_messages() {
    fn last(messages: &[AgentMessage]) -> Option<&AgentMessage> {
        messages.last()
    }
    fn push(messages: &mut Vec<AgentMessage>) {
        messages.push(user("more"));
    }

    let mut pruned = Pruned::new(vec![user("hi")], &budget(10_000, 2));
    push(&mut pruned);
    assert_eq!(pruned.len(), 2);
    assert!(matches!(last(&pruned), Some(AgentMessage::User { content, .. }) if content == "more"));
    assert_eq!(std::mem::take(&mut *pruned).len(), 2);
    assert!(pruned.is_empty());
    assert_eq!(pruned.dropped(), 0, "taking the messages is not a prune");
}
