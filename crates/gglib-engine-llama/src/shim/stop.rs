//! A request's `stop` strings, found in the reply as it streams.

use serde_json::Value;

/// Holds back the end of the reply that could begin a stop string, and
/// cuts the reply where one is found, as llama-server's word stop does:
/// the stop string itself is not sent.
#[derive(Debug, Clone, Default)]
pub(crate) struct StopScanner {
    stops: Vec<String>,
    held: String,
    stopped: bool,
}

impl StopScanner {
    /// The scanner for a request's `stop`: one string or an array of them.
    /// Empty strings stop nothing.
    pub(crate) fn from_request(body: &Value) -> Self {
        let stops = match body.get("stop") {
            Some(Value::String(one)) => vec![one.clone()],
            Some(Value::Array(many)) => many
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        Self {
            stops: stops.into_iter().filter(|s| !s.is_empty()).collect(),
            ..Self::default()
        }
    }

    /// Whether a stop string has been found.
    pub(crate) fn stopped(&self) -> bool {
        self.stopped
    }

    /// The text of `piece` that can be sent now. Once a stop string is
    /// found, nothing more is.
    pub(crate) fn push(&mut self, piece: &str) -> String {
        if self.stopped {
            return String::new();
        }
        let mut text = std::mem::take(&mut self.held);
        text.push_str(piece);
        if let Some(at) = self
            .stops
            .iter()
            .filter_map(|s| text.find(s.as_str()))
            .min()
        {
            self.stopped = true;
            text.truncate(at);
            return text;
        }
        let keep = self.partial_suffix(&text);
        self.held = text.split_off(keep);
        text
    }

    /// What was held back, once the reply has ended without a stop string.
    pub(crate) fn flush(&mut self) -> String {
        std::mem::take(&mut self.held)
    }

    /// Where the longest end of `text` that begins a stop string starts;
    /// `text.len()` when no end does.
    fn partial_suffix(&self, text: &str) -> usize {
        text.char_indices()
            .map(|(i, _)| i)
            .find(|&i| self.stops.iter().any(|s| s.starts_with(&text[i..])))
            .unwrap_or(text.len())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scanner(stop: &Value) -> StopScanner {
        StopScanner::from_request(&json!({"stop": stop}))
    }

    #[test]
    fn a_stop_split_across_pieces_is_cut_and_not_sent() {
        let mut s = scanner(&json!(["</end>"]));
        assert_eq!(s.push("one </e"), "one ");
        assert_eq!(s.push("nd> two"), "");
        assert!(s.stopped());
        assert_eq!(s.push("three"), "");
    }

    #[test]
    fn a_held_beginning_that_does_not_complete_is_sent() {
        let mut s = scanner(&json!("STOP"));
        assert_eq!(s.push("ST"), "");
        assert_eq!(s.push("AR"), "STAR");
        assert_eq!(s.push("S"), "");
        assert_eq!(s.flush(), "S");
        assert!(!s.stopped());
    }

    #[test]
    fn the_earliest_of_several_stops_wins() {
        let mut s = scanner(&json!(["b", "a"]));
        assert_eq!(s.push("xxab"), "xx");
    }

    #[test]
    fn no_stop_passes_everything_through_at_once() {
        let mut s = StopScanner::from_request(&json!({}));
        assert_eq!(s.push("é and more"), "é and more");
        assert_eq!(s.flush(), "");
    }
}
