# gglib-broadcast

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-broadcast-complexity.json)

Layer: **utility** · [workspace diagram](../README.md#architecture-overview)
<!-- crate-header:end -->

Generic Server-Sent Events broadcast utility shared by `gglib-axum` and `gglib-proxy`.
It sends; it parses nothing. The decoder and encoder for chat-completion streams
are `gglib_core::sse`.

Streaming is the default shape of everything users watch here — tokens, download
progress, dashboard updates — so the fan-out lives in one dependency-free place
rather than being reimplemented per surface.

## Architecture

This crate is a pure, dependency-free **leaf** utility - it depends on nothing but `axum`'s SSE
response types, `tokio`, `tokio-stream`, `futures-util`, and `serde`/`serde_json`. Critically, it
does **not** depend on any other `gglib-*` crate, which is what allows both an Adapter-layer crate
(`gglib-axum`) and an Infrastructure-layer crate (`gglib-proxy`) to depend on it directly without
inverting the hexagonal layering rules enforced by `scripts/check_boundaries.sh`.

## Internal Structure

```text
┌───────────────────────────────────────────────────────────────┐
│                        gglib-broadcast                        │
├───────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                          lib.rs                         │  │
│  │  Broadcaster<T>   - generic broadcast::Sender<T> wrapper│  │
│  │  SseOptions       - keep-alive interval/text config     │  │
│  │  subscribe_until()        - live events only            │  │
│  │  subscribe_with_hydration_until() - snapshot + live     │  │
│  │  subscribe_events()       - raw events, tests only      │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
```

## Usage

```rust,ignore
use std::sync::Arc;
use gglib_broadcast::{Broadcaster, SseOptions};
use serde::Serialize;

#[derive(Clone, Serialize)]
struct MyEvent { message: String }

let broadcaster = Arc::new(Broadcaster::<MyEvent>::new(256));

// In an Axum handler:
// broadcaster.clone().subscribe_until(SseOptions::default(), shutdown)
// or, to send a full-state snapshot before streaming live updates:
// broadcaster.clone().subscribe_with_hydration_until(current_snapshot, SseOptions::default(), shutdown)

broadcaster.send(MyEvent { message: "hello".into() });
```
