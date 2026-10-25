#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

//! A test's fake HTTP server takes its connections from here, and gets each
//! one only with the request already read.
//!
//! A fake that writes its answer before it reads the request races the
//! client: the answer can reach hyper before hyper has sent the request, and
//! hyper cancels it (`hyper::Error(UnexpectedMessage)`). tokio 1.54 made the
//! client lose that race far more often, because an accepted socket now starts
//! writable and the first write goes out at once (tokio-rs/tokio#8435): two
//! benchmark tests lost it on Linux CI (#1553), and in about one run in five
//! on a Mac. A fake that answers and drops the socket with part of the request
//! unread can reset the connection, and the client can lose the answer. Both
//! are avoided the same way: read the whole request first. A fake built on
//! these functions gets no stream to write to until its request is read, so
//! it cannot forget.
//!
//! [`accept`] takes a connection and reads its request. [`incoming`] only
//! takes the connection, for a server that reads in a task of its own, or a
//! test about the connection itself; the [`Incoming`] it returns gives no
//! stream until [`Incoming::request`] has read one. [`blocking`] is the same
//! for `std::net`.

use std::io;

use tokio::io::AsyncReadExt as _;
use tokio::net::{TcpListener, TcpStream};

pub mod blocking;
mod request;

pub use request::Request;
use request::{Body, chunk_size, invalid, parse_head, push_head_byte, push_line_byte, room_for};

/// A connection taken from a listener whose request has not been read yet.
///
/// It owns the stream and borrows nothing, so it can move into a spawned
/// task. Holding it, or dropping it, never writes a byte.
#[derive(Debug)]
pub struct Incoming(TcpStream);

impl Incoming {
    /// Read this connection's request whole, and hand back the stream to
    /// answer on.
    ///
    /// # Errors
    ///
    /// `UnexpectedEof` when the client closes before the request ends,
    /// including before its first byte. `InvalidData` when the head is not a
    /// request or a length is not one this reader will hold.
    pub async fn request(mut self) -> io::Result<(Request, TcpStream)> {
        let request = read_request(&mut self.0).await?;
        Ok((request, self.0))
    }
}

/// Take the next connection and read its request whole.
///
/// # Errors
///
/// The listener's own error, or any error of [`Incoming::request`].
pub async fn accept(listener: &TcpListener) -> io::Result<(Request, TcpStream)> {
    incoming(listener).await?.request().await
}

/// Take the next connection without reading from it.
///
/// # Errors
///
/// The listener's own error.
pub async fn incoming(listener: &TcpListener) -> io::Result<Incoming> {
    let (stream, _) = listener.accept().await?;
    Ok(Incoming(stream))
}

/// The head a byte at a time, then exactly the body, so nothing after the
/// request is consumed: a test can go on reading the stream to see the client
/// close it.
async fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut head = Vec::new();
    while !push_head_byte(&mut head, stream.read_u8().await?)? {}
    let (mut request, body) = parse_head(&head)?;
    request.body = match body {
        Body::None => Vec::new(),
        Body::Length(length) => {
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await?;
            body
        }
        Body::Chunked => read_chunks(stream).await?,
    };
    Ok(request)
}

async fn read_chunks(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let size = chunk_size(&read_line(stream).await?)?;
        if size == 0 {
            break;
        }
        let start = body.len();
        body.resize(room_for(start, size)?, 0);
        stream.read_exact(&mut body[start..]).await?;
        if !read_line(stream).await?.is_empty() {
            return Err(invalid("a chunk ran past its size"));
        }
    }
    // Any trailers, then the blank line that ends the body.
    while !read_line(stream).await?.is_empty() {}
    Ok(body)
}

async fn read_line(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    while !push_line_byte(&mut line, stream.read_u8().await?)? {}
    Ok(line)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
