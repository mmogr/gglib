# gglib-sse

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-sse-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-sse-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-sse-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-sse-complexity.json)

Generic Server-Sent Events broadcast utility shared by `gglib-axum` and `gglib-proxy`.

## Architecture

This crate is a pure, dependency-free **leaf** utility - it depends on nothing but `axum`'s SSE
response types, `tokio`, `tokio-stream`, `futures-util`, and `serde`/`serde_json`. Critically, it
does **not** depend on any other `gglib-*` crate, which is what allows both an Adapter-layer crate
(`gglib-axum`) and an Infrastructure-layer crate (`gglib-proxy`) to depend on it directly without
inverting the hexagonal layering rules enforced by `scripts/check_boundaries.sh`.

```text
        gglib-axum (adapter)              gglib-proxy (infrastructure)
        ┌──────────────────┐              ┌──────────────────┐
        │  SseBroadcaster   │              │ DashboardState    │
        │  wraps            │              │  wraps            │
        └─────────┬─────────┘              └─────────┬─────────┘
                  │                                   │
                  └───────────────┬───────────────────┘
                                  ▼
                        ┌──────────────────┐
                        │    gglib-sse      │
                        │  Broadcaster<T>   │
                        └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete workspace diagram.

## Internal Structure

```text
┌───────────────────────────────────────────────────────────────┐
│                          gglib-sse                             │
├───────────────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────────────────┐   │
│  │                       lib.rs                            │   │
│  │  Broadcaster<T>   - generic broadcast::Sender<T> wrapper│   │
│  │  SseOptions       - keep-alive interval/text config      │   │
│  │  subscribe()              - live events only             │   │
│  │  subscribe_with_hydration() - initial snapshot + live     │   │
│  └────────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────┘
```

## Usage

```rust,ignore
use std::sync::Arc;
use gglib_sse::{Broadcaster, SseOptions};
use serde::Serialize;

#[derive(Clone, Serialize)]
struct MyEvent { message: String }

let broadcaster = Arc::new(Broadcaster::<MyEvent>::new(256));

// In an Axum handler:
// broadcaster.clone().subscribe(SseOptions::default())
// or, to send a full-state snapshot before streaming live updates:
// broadcaster.clone().subscribe_with_hydration(current_snapshot, SseOptions::default())

broadcaster.send(MyEvent { message: "hello".into() });
```
