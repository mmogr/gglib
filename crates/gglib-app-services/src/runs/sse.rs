//! The `data:` payloads of a server-sent event stream, as bytes arrive.
//!
//! Bytes are buffered until an event's blank line, so a character split
//! across two reads is decoded whole. `data:` lines within one event are
//! joined with a newline, per the SSE spec; comments and other fields are
//! skipped.

/// Splits a byte stream into the data of each complete event.
#[derive(Default)]
pub(super) struct DataFrames {
    buffer: Vec<u8>,
}

impl DataFrames {
    /// Take `chunk`, and return the data of every event it completed.
    pub(super) fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some((end, gap)) = event_end(&self.buffer) {
            let event: Vec<u8> = self.buffer.drain(..end + gap).collect();
            let text = String::from_utf8_lossy(&event[..end]);
            let data: Vec<&str> = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|rest| rest.strip_prefix(' ').unwrap_or(rest))
                .collect();
            if !data.is_empty() {
                out.push(data.join("\n"));
            }
        }
        out
    }
}

/// Where the first event ends, and how long its blank-line terminator is.
fn event_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find(buffer, b"\n\n").map(|i| (i, 2));
    let crlf = find(buffer, b"\r\n\r\n").map(|i| (i, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_is_taken_only_once_its_blank_line_arrives() {
        let mut frames = DataFrames::default();
        assert!(frames.push(b"data: {\"a\":").is_empty());
        assert_eq!(frames.push(b"1}\n\ndata: [DONE]"), ["{\"a\":1}"]);
        assert_eq!(frames.push(b"\n\n"), ["[DONE]"]);
    }

    #[test]
    fn comments_crlf_and_multi_line_data_are_handled() {
        let mut frames = DataFrames::default();
        let got = frames.push(b": keep-alive\n\nid: 3\r\ndata: one\r\ndata:two\r\n\r\n");
        assert_eq!(got, ["one\ntwo"]);
    }

    #[test]
    fn a_character_split_across_reads_is_decoded_whole() {
        let mut frames = DataFrames::default();
        let bytes = "data: caf\u{e9}\n\n".as_bytes();
        assert!(frames.push(&bytes[..10]).is_empty());
        assert_eq!(frames.push(&bytes[10..]), ["caf\u{e9}"]);
    }
}
