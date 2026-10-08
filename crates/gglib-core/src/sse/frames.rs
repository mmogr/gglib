//! A server-sent event stream cut into lines, and into the `data:` payload
//! of each event, as bytes arrive.
//!
//! [`Lines`] cuts the bytes, for [`DataFrames`] and for
//! [`super::SseStreamDecoder`] alike: at `\n`, with any `\r` before it
//! dropped. Bytes after the last `\n` wait for their line to end, so a
//! character split across two reads is decoded whole.
//!
//! [`DataFrames`] reads events off those lines. `data:` lines within one
//! event are joined with a newline, per the SSE spec; `id:` and `event:` are
//! kept for a caller that asks for the whole [`Event`]; comments and other
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

/// One complete event that carried data. Nothing in it comes from the event
/// before.
#[derive(Debug, PartialEq, Eq)]
pub struct Event {
    /// Its `id:` field.
    pub id: Option<String>,
    /// Its `event:` field.
    pub name: Option<String>,
    /// Its `data:` lines, joined with a newline.
    pub data: String,
}

/// What follows `field:` on `line`, less the one space the colon may have
/// after it.
fn value<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(field)?.strip_prefix(':')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// Splits a byte stream into the data of each complete event.
pub struct DataFrames {
    lines: Lines,
    /// The `id:` of the event that has not ended yet.
    id: Option<String>,
    /// Its `event:`.
    name: Option<String>,
    /// Its `data:` payloads.
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
            id: None,
            name: None,
            data: Vec::new(),
            taken: 0,
            limit,
        }
    }

    /// A splitter with no limit, for a stream whose sender is trusted not to
    /// send an event without end.
    pub const fn unbounded() -> Self {
        Self::new(usize::MAX)
    }

    /// Whether the incomplete event now held is past the limit, counting
    /// every byte it has taken on the wire so far.
    pub const fn overflowed(&self) -> bool {
        self.taken + self.lines.held() > self.limit
    }

    /// Take `chunk`, and return the data of every event it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.push_events(chunk)
            .into_iter()
            .map(|event| event.data)
            .collect()
    }

    /// Take `chunk`, and return every event it completed, with its fields.
    pub fn push_events(&mut self, chunk: &[u8]) -> Vec<Event> {
        self.lines.push(chunk);
        let mut out = Vec::new();
        loop {
            let held = self.lines.held();
            let Some(line) = self.lines.next_line() else {
                break;
            };
            if line.is_empty() {
                self.taken = 0;
                let (id, name) = (self.id.take(), self.name.take());
                if !self.data.is_empty() {
                    let data = self.data.join("\n");
                    self.data.clear();
                    out.push(Event { id, name, data });
                }
                continue;
            }
            self.taken += held - self.lines.held();
            let text = String::from_utf8_lossy(&line);
            if let Some(data) = value(&text, "data") {
                self.data.push(data.to_owned());
            } else if let Some(id) = value(&text, "id") {
                self.id = Some(id.to_owned());
            } else if let Some(name) = value(&text, "event") {
                self.name = Some(name.to_owned());
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
