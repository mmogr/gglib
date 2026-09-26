# `proxy_dashboard`

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-proxy_dashboard-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-proxy_dashboard-complexity.json)

<!-- module-docs:start -->

`gglib proxy dashboard` — a live terminal view of an already-running proxy.

Three concerns, along the line that makes the interesting one testable:

| Concern | Files |
|---------|-------|
| The IO: connect, read the SSE stream, move the cursor, restore the terminal | `mod.rs` |
| The server's JSON contract, mirrored `Deserialize`-only | `wire.rs`, and `wire_sampling.rs` for the sampling readback |
| Snapshot → the text of one frame. Pure, and where nearly all the tests are | `render.rs`, with two sections in files of their own: `render_reasoning.rs` and `render_defects.rs` |

`render.rs` takes terminal width as an argument rather than asking the
terminal. In cooked mode a long line wraps onto another physical row, and the
next frame decides how far to move the cursor up from `visual_row_count` — a
renderer that measured a different width than the one it drew for would leave
the frame smeared.

`wire.rs` is deliberately tolerant in both directions: no `deny_unknown_fields`,
so a newer proxy's extra fields are ignored, and `#[serde(default)]` throughout,
so an older proxy's missing ones read as zero. This dashboard is routinely
pointed at a proxy from a different build.

## Redraw strategy: cursor movement, not raw mode

Earlier CLI work in this crate (see
[`crate::handlers::model::download::run_interactive_monitor`]) already
established
that `crossterm::terminal::enable_raw_mode()` breaks `println!`-based
redraws (it disables `OPOST`, so `\n` stops returning the cursor to column
0). This module never touches raw mode. Instead, each frame after the
first moves the cursor up by the previous frame's *physical row* count
(see [`visual_row_count`]) and clears everything below before printing
the next frame — plain `crossterm::cursor`/`terminal` commands in normal
(cooked) mode, which compose fine with ordinary `print!`/`println!`.
Cooked mode means a line longer than the terminal's width auto-wraps onto
an extra physical row, which is exactly what `visual_row_count` accounts
for when computing how far to move the cursor up on the next tick. When
stdout is not a TTY (piped output, CI), frames are printed sequentially
instead, since there is no cursor to move.

## Shutdown

`Ctrl+C` is raced directly against each stream-chunk read via
`tokio::select!`, so it is handled between chunks rather than only after a
full frame arrives. [`TerminalGuard`] hides the cursor for the duration of
the dashboard and unconditionally restores it (and prints a trailing
newline) on drop — including on the `Ctrl+C` path, an early `?` return, or
a panic — so the terminal is never left in a half-drawn state. Dropping
the `reqwest` response stream (which happens automatically once
`execute()` returns) closes the underlying SSE connection.

<!-- module-docs:end -->
