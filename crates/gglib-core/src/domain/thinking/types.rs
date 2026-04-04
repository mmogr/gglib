//! Public data types for thinking/reasoning content.

/// Result of parsing a complete message for thinking content.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedThinkingContent {
    /// The thinking/reasoning content, or `None` if none was found.
    pub thinking: Option<String>,
    /// The main content with thinking tags removed.
    pub content: String,
    /// Duration in seconds if a `duration="…"` attribute was present.
    pub duration_seconds: Option<f64>,
}

/// Events emitted by [`super::ThinkingAccumulator`] as streamed text arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThinkingEvent {
    /// A chunk of thinking/reasoning content.
    ThinkingDelta(String),
    /// The thinking phase has ended (closing tag received).
    ThinkingEnd,
    /// A chunk of normal (non-thinking) content.
    ContentDelta(String),
}
