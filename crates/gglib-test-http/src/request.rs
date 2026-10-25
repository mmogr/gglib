//! The request a fake server was sent, and the byte rules both readers share.
//!
//! The two readers (the tokio one in `lib.rs`, the std one in `blocking.rs`)
//! each have their own read loop, because one awaits and one blocks. What a
//! head, a length or a chunk line means is decided here, once; the loops are
//! the same shape, and each reader's tests run every path through its own
//! loop: a body by length, several chunks with an extension and trailers, an
//! over-long chunk, a head refused here, a head over the cap, and early ends.

use std::io;

/// The most a head may hold before it is refused. Far above any request a
/// test sends; it exists so a client that never ends its head is an error and
/// not a reader that grows without bound.
pub(crate) const MAX_HEAD: usize = 64 * 1024;

/// The most a body may hold. A bad length is an error, not an allocation the
/// size of the number.
pub(crate) const MAX_BODY: usize = 512 * 1024 * 1024;

/// One request, read whole: the head through its blank line, then the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method, as sent: `GET`, `POST`.
    pub method: String,
    /// The request target exactly as sent: the query, `%`-escapes and an
    /// absolute form are kept, so a test can assert the line the client wrote.
    pub path: String,
    /// The head as text, from the request line through the `\r\n\r\n` that
    /// ends it, with any invalid UTF-8 replaced.
    pub head: String,
    /// Each header line in order, the name as sent and the value trimmed.
    pub headers: Vec<(String, String)>,
    /// The body: `content-length` bytes, or the decoded chunks of a
    /// `transfer-encoding: chunked` body. Empty when the head names neither.
    pub body: Vec<u8>,
}

impl Request {
    /// The request line without its `\r\n`: `POST /v1/chat/completions HTTP/1.1`.
    #[must_use]
    pub fn line(&self) -> &str {
        self.head.split("\r\n").next().unwrap_or_default()
    }

    /// The first header of this name, whatever its case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(sent, _)| sent.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The body as text, any invalid UTF-8 replaced.
    #[must_use]
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// How the body after a head is framed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Body {
    None,
    Length(usize),
    Chunked,
}

pub(crate) fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

/// Add one byte to a head being read. `true` once the head is whole.
pub(crate) fn push_head_byte(head: &mut Vec<u8>, byte: u8) -> io::Result<bool> {
    head.push(byte);
    if head.ends_with(b"\r\n\r\n") {
        return Ok(true);
    }
    if head.len() >= MAX_HEAD {
        return Err(invalid("the request head never ended"));
    }
    Ok(false)
}

/// Add one byte to a chunk-size or trailer line. `true` once the line is
/// whole; its `\r\n` is then removed.
pub(crate) fn push_line_byte(line: &mut Vec<u8>, byte: u8) -> io::Result<bool> {
    line.push(byte);
    if line.ends_with(b"\r\n") {
        line.truncate(line.len() - 2);
        return Ok(true);
    }
    if line.len() >= MAX_HEAD {
        return Err(invalid("a chunk line never ended"));
    }
    Ok(false)
}

/// The request a whole head describes, with an empty body, and how its body
/// is framed. A chunked body wins over a `content-length`, as HTTP/1.1 says.
pub(crate) fn parse_head(raw: &[u8]) -> io::Result<(Request, Body)> {
    let head = String::from_utf8_lossy(raw).into_owned();
    let mut lines = head.split("\r\n");
    let mut words = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(path)) = (words.next(), words.next()) else {
        return Err(invalid("the request line has no target"));
    };
    if method.is_empty() || path.is_empty() {
        return Err(invalid("the request line has no target"));
    }
    let (method, path) = (method.to_owned(), path.to_owned());

    let mut headers = Vec::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(invalid("a header line has no colon"));
        };
        // A name with space before its colon is refused rather than kept
        // under a name no lookup would find, which would leave its body unread.
        if name.is_empty() || name.ends_with(char::is_whitespace) {
            return Err(invalid("a header name is empty or ends in space"));
        }
        headers.push((name.to_owned(), value.trim().to_owned()));
    }

    let request = Request {
        method,
        path,
        head,
        headers,
        body: Vec::new(),
    };
    let body = if values(&request, "transfer-encoding")
        .flat_map(|value| value.split(','))
        .any(|coding| coding.trim().eq_ignore_ascii_case("chunked"))
    {
        Body::Chunked
    } else if let Some(length) = content_length(&request)? {
        Body::Length(length)
    } else {
        Body::None
    };
    Ok((request, body))
}

/// Every value of a header that may be sent more than once.
fn values<'a>(request: &'a Request, name: &'a str) -> impl Iterator<Item = &'a str> {
    request
        .headers
        .iter()
        .filter(move |(sent, _)| sent.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// The body's length, if the head names one. Digits only, and two lengths
/// that disagree are refused rather than one of them believed.
fn content_length(request: &Request) -> io::Result<Option<usize>> {
    let mut length = None;
    for value in values(request, "content-length") {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid("a content-length is not a number"));
        }
        let value = body_size(value.parse().ok())?;
        if length.is_some_and(|seen| seen != value) {
            return Err(invalid("two content-lengths disagree"));
        }
        length = Some(value);
    }
    Ok(length)
}

/// The size a chunk line names, in hex, before any `;` extension.
pub(crate) fn chunk_size(line: &[u8]) -> io::Result<usize> {
    let line = String::from_utf8_lossy(line);
    let digits = line.split(';').next().unwrap_or_default().trim();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid("a chunk size is not hex"));
    }
    body_size(usize::from_str_radix(digits, 16).ok())
}

/// Room for `more` bytes on a body that already holds `held`.
pub(crate) fn room_for(held: usize, more: usize) -> io::Result<usize> {
    body_size(held.checked_add(more))
}

fn body_size(size: Option<usize>) -> io::Result<usize> {
    size.filter(|size| *size <= MAX_BODY)
        .ok_or_else(|| invalid("a body length is not a number this reader will hold"))
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
