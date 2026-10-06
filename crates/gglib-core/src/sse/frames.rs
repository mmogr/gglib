//! A server-sent event stream cut into lines, and into the `data:` payload
//! of each event, as bytes arrive.
//!
//! [`Lines`] cuts the bytes, for [`DataFrames`] and for
//! [`super::SseStreamDecoder`] alike: at `\n`, with any `\r` before it
//! dropped. Bytes after the last `\n` wait for their line to end, so a
//! character split across two reads is decoded whole.
//!
//! [`DataFrames`] reads events off those lines. `data:` lines within one
//! event are joined with a newline, per the SSE spec; comments and other
//! fields are skipped. An event still incomplete past a limit is not
//! buffered further: [`DataFrames::overflowed`] says so, and the caller
//! stops.

/// Splits a byte stream into its complete lines, still as bytes.
#[derive(Default)]
pub(super) struct Lines {
    /// Every byte not yet returned as a line; past the last `\n`, the start
    /// of a line whose end has not arrived.
    buffer: Vec<u8>,
}

impl Lines {
    pub(super) const fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Take `chunk`.
    pub(super) fn push(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// The next complete line, without its `\n` and any `\r` before it.
    pub(super) fn next_line(&mut self) -> Option<Vec<u8>> {
        let end = self.buffer.iter().position(|&byte| byte == b'\n')?;
        let mut line: Vec<u8> = self.buffer.drain(..=end).collect();
        line.pop();
        while line.last() == Some(&b'\r') {
            line.pop();
        }
        Some(line)
    }

    /// How many bytes are held: the lines not yet returned, whole or not.
    pub(super) const fn held(&self) -> usize {
        self.buffer.len()
    }
}

/// Splits a byte stream into the data of each complete event.
pub struct DataFrames {
    lines: Lines,
    /// The `data:` payloads of the event that has not ended yet.
    data: Vec<String>,
    /// How many bytes that event's complete lines took on the wire.
    taken: usize,
    limit: usize,
}

impl DataFrames {
    /// A splitter that holds at most `limit` bytes of an incomplete event.
    pub const fn new(limit: usize) -> Self {
        Self {
            lines: Lines::new(),
            data: Vec::new(),
            taken: 0,
            limit,
        }
    }

    /// Whether the incomplete event now held is past the limit, counting
    /// every byte it has taken on the wire so far.
    pub const fn overflowed(&self) -> bool {
        self.taken + self.lines.held() > self.limit
    }

    /// Take `chunk`, and return the data of every event it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.lines.push(chunk);
        let mut out = Vec::new();
        loop {
            let held = self.lines.held();
            let Some(line) = self.lines.next_line() else {
                break;
            };
            if line.is_empty() {
                self.taken = 0;
                if !self.data.is_empty() {
                    out.push(self.data.join("\n"));
                    self.data.clear();
                }
                continue;
            }
            self.taken += held - self.lines.held();
            let text = String::from_utf8_lossy(&line);
            if let Some(rest) = text.strip_prefix("data:") {
                self.data
                    .push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
