# proxy_dashboard

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

<!-- module-docs:end -->
