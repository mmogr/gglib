//! What gglib did to one request on its way through the endpoint, in the
//! words every surface shows.
//!
//! The proxy's request ledger carries one list per request. Each entry is
//! the kind, with what it needs to say, beside the word a person reads,
//! written once here: the CLI dashboard prints the word, the GUI's Recent
//! ledger shows it, and neither spells it for itself.

use serde::Serialize;

use super::defect_counts::LoopGuardTrip;

/// One thing gglib did to a request: what it was, and the word for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct Intervention {
    /// What was done.
    pub what: InterventionKind,
    /// The word a person reads, from [`InterventionKind::word`].
    pub word: String,
}

impl Intervention {
    /// `what`, with its word.
    #[must_use]
    pub fn new(what: InterventionKind) -> Self {
        let word = what.word();
        Self { what, word }
    }
}

/// The kinds of thing gglib does to a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InterventionKind {
    /// The turn's tool call failed its schema and a re-issue produced one
    /// that conforms.
    RepairedToolCall,
    /// Old messages were replaced with the truncation placeholder so the
    /// conversation fit the context.
    Trimmed {
        /// How many messages.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        messages: usize,
    },
    /// A decode-time grammar held the output to a tool call.
    Constrained,
    /// The loop guard tripped and forwarded the request with a note.
    NotedLoop {
        /// The detector that tripped.
        detector: LoopGuardTrip,
    },
    /// The loop guard ended the run itself: the same run tripped again after
    /// its note (ADR 0021).
    StoppedLoop {
        /// The detector that tripped.
        detector: LoopGuardTrip,
    },
    /// The request was refused before it was forwarded.
    Refused {
        /// Why.
        reason: Refusal,
    },
}

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Refusal {
    /// The conversation could not be trimmed to fit the context.
    LongerThanContext,
}

impl InterventionKind {
    /// The word a person reads.
    #[must_use]
    pub fn word(&self) -> String {
        match self {
            Self::RepairedToolCall => "Repaired a tool call".to_owned(),
            Self::Trimmed { messages: 1 } => "Trimmed 1 old message".to_owned(),
            Self::Trimmed { messages } => format!("Trimmed {messages} old messages"),
            Self::Constrained => "Constrained tool calls".to_owned(),
            Self::NotedLoop { detector } => format!("Noted {}", tripped(*detector)),
            Self::StoppedLoop { detector } => format!("Stopped {}", tripped(*detector)),
            Self::Refused {
                reason: Refusal::LongerThanContext,
            } => "Refused: longer than the context".to_owned(),
        }
    }
}

/// What a detector's trip is called in a sentence.
const fn tripped(detector: LoopGuardTrip) -> &'static str {
    match detector {
        LoopGuardTrip::Loop => "a loop",
        LoopGuardTrip::Stagnation => "stagnation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_the_word_the_endpoint_place_shows() {
        let words: Vec<String> = [
            InterventionKind::RepairedToolCall,
            InterventionKind::Trimmed { messages: 4 },
            InterventionKind::Trimmed { messages: 1 },
            InterventionKind::Constrained,
            InterventionKind::NotedLoop {
                detector: LoopGuardTrip::Loop,
            },
            InterventionKind::StoppedLoop {
                detector: LoopGuardTrip::Stagnation,
            },
            InterventionKind::Refused {
                reason: Refusal::LongerThanContext,
            },
        ]
        .into_iter()
        .map(|what| Intervention::new(what).word)
        .collect();
        assert_eq!(
            words,
            [
                "Repaired a tool call",
                "Trimmed 4 old messages",
                "Trimmed 1 old message",
                "Constrained tool calls",
                "Noted a loop",
                "Stopped stagnation",
                "Refused: longer than the context",
            ]
        );
    }

    #[test]
    fn the_wire_carries_the_kind_tagged_beside_the_word() {
        let json = serde_json::to_string(&Intervention::new(InterventionKind::Trimmed {
            messages: 4,
        }))
        .unwrap();
        assert_eq!(
            json,
            r#"{"what":{"kind":"trimmed","messages":4},"word":"Trimmed 4 old messages"}"#
        );
        let json = serde_json::to_string(&Intervention::new(InterventionKind::Refused {
            reason: Refusal::LongerThanContext,
        }))
        .unwrap();
        assert_eq!(
            json,
            r#"{"what":{"kind":"refused","reason":{"kind":"longer_than_context"}},"word":"Refused: longer than the context"}"#
        );
    }
}
