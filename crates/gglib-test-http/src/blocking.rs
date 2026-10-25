//! The same as the crate root, for a fake server on `std::net`.
//!
//! The rules are the crate root's: a stream is handed out only with its
//! request read, the head a byte at a time and then exactly the body.

use std::io::{self, Read as _};
use std::net::{TcpListener, TcpStream};

use crate::request::{
    Body, Request, chunk_size, invalid, parse_head, push_head_byte, push_line_byte, room_for,
};

/// A connection taken from a listener whose request has not been read yet.
/// Holding it, or dropping it, never writes a byte.
#[derive(Debug)]
pub struct Incoming(TcpStream);

impl Incoming {
    /// Read this connection's request whole, and hand back the stream to
    /// answer on.
    ///
    /// # Errors
    ///
    /// As [`crate::Incoming::request`].
    pub fn request(mut self) -> io::Result<(Request, TcpStream)> {
        let request = read_request(&mut self.0)?;
        Ok((request, self.0))
    }
}

/// Take the next connection and read its request whole.
///
/// # Errors
///
/// The listener's own error, or any error of [`Incoming::request`].
pub fn accept(listener: &TcpListener) -> io::Result<(Request, TcpStream)> {
    incoming(listener)?.request()
}

/// Take the next connection without reading from it.
///
/// # Errors
///
/// The listener's own error.
pub fn incoming(listener: &TcpListener) -> io::Result<Incoming> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the one place a connection is taken, and it gives no stream until the request is read"
    )]
    let (stream, _) = listener.accept()?;
    Ok(Incoming(stream))
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut head = Vec::new();
    while !push_head_byte(&mut head, read_byte(stream)?)? {}
    let (mut request, body) = parse_head(&head)?;
    request.body = match body {
        Body::None => Vec::new(),
        Body::Length(length) => {
            let mut body = vec![0; length];
            stream.read_exact(&mut body)?;
            body
        }
        Body::Chunked => read_chunks(stream)?,
    };
    Ok(request)
}

fn read_chunks(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let size = chunk_size(&read_line(stream)?)?;
        if size == 0 {
            break;
        }
        let start = body.len();
        body.resize(room_for(start, size)?, 0);
        stream.read_exact(&mut body[start..])?;
        if !read_line(stream)?.is_empty() {
            return Err(invalid("a chunk ran past its size"));
        }
    }
    // Any trailers, then the blank line that ends the body.
    while !read_line(stream)?.is_empty() {}
    Ok(body)
}

fn read_line(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    while !push_line_byte(&mut line, read_byte(stream)?)? {}
    Ok(line)
}

fn read_byte(stream: &mut TcpStream) -> io::Result<u8> {
    let mut byte = [0];
    stream.read_exact(&mut byte)?;
    Ok(byte[0])
}

#[cfg(test)]
#[path = "blocking_tests.rs"]
mod tests;
