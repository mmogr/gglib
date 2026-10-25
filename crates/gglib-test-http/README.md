# gglib-test-http

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-http-tests.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-http-loc.json)

The connection a test's fake HTTP server answers on, handed over only once the
request has been read whole.

A fake that answers before it has read the request races its client: the
answer can reach hyper before hyper has sent the request, and hyper cancels it
with `hyper::Error(UnexpectedMessage)`. tokio 1.54 made the client lose that
race far more often, because an accepted socket now starts writable and the
first write goes out at once
([tokio-rs/tokio#8435](https://github.com/tokio-rs/tokio/pull/8435)). That is
how a tokio bump failed two benchmark tests on Linux CI (#1553); on a Mac,
about one run in five failed one of them. A fake that answers and drops the
socket with part of the request unread can instead reset the connection, and
the client loses the answer. A fake built on this crate can do neither, and
`clippy.toml` refuses a raw `TcpListener::accept` or `incoming` anywhere else
in the workspace.

## Architecture

A leaf, taken only from `[dev-dependencies]`. It depends on no `gglib-*` crate
and on nothing but tokio's `net` and `io-util`: it reads bytes and knows no
HTTP library. Crates that may not depend on hyper or axum directly,
dev-dependencies included (`gglib-cli`, `gglib-app-services`, `gglib-runtime`,
`gglib-download` and `gglib-hf` among them), can use it in their tests. That
check looks one level deep, so this crate is held to the same ban rather than
becoming a way round it.

```text
  a test's fake server                      gglib-test-http
  ┌──────────────────────┐   accept()      ┌──────────────────────────────┐
  │ binds 127.0.0.1:0     │ ─────────────▶ │ takes the connection          │
  │                       │                │ reads the head, byte by byte  │
  │ answers on the stream │ ◀───────────── │ reads exactly the body        │
  └──────────────────────┘ (Request, stream)└──────────────────────────────┘
```

## Internal Structure

```text
┌──────────────────────────────────────────────────────────────────┐
│                         gglib-test-http                           │
├──────────────────────────────────────────────────────────────────┤
│  lib.rs       accept · incoming · Incoming::request   (tokio)     │
│  blocking.rs  the same three over std::net                         │
│  request.rs   Request · the byte rules both readers share          │
└──────────────────────────────────────────────────────────────────┘
```

- [`src/lib.rs`](src/lib.rs): `accept` takes a connection and reads its
  request; `incoming` only takes it, for a server that reads in a task of its
  own or a test about the connection itself. The `Incoming` it returns gives no
  stream until `request()` has read one, so holding or dropping it writes
  nothing.
- [`src/blocking.rs`](src/blocking.rs): the same for `std::net`.
- [`src/request.rs`](src/request.rs): `Request` and the rules for its bytes.
  The head is read a byte at a time up to its blank line, then exactly the
  body (`content-length`, or decoded chunks and their trailers), so nothing
  after the request is consumed and a test can go on reading the stream.

## Usage

```rust,ignore
use gglib_test_http::accept;
use tokio::io::AsyncWriteExt as _;

let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
let (request, mut stream) = accept(&listener).await?;
assert_eq!(request.line(), "POST /v1/chat/completions HTTP/1.1");
stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}").await?;
```
