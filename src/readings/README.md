# readings

<!-- module-docs:start -->

Pure readers the contract tests pin: data in, a reading out, no React and no
IO. Each has a test under `tests/ts/contracts/` that holds it to the Rust
source or a checked-in contract file, which is why they live together and
apart from the components that show them.

| File | Purpose |
|------|---------|
| `contextReading.ts` | The conversation's context reading, from the figures of the reply that decides it (the newest, passing over an unfinished one without counts), and the sentences its detail says; nothing when a figure is missing. The rule of `contracts/context/readings.json` |
| `turnFigures.ts` | Which figures a turn's margin shows: only those the page has for that turn (model, quantisation, thought, tool calls, tokens read and cached, time and a computed rate), never a zero or a dash for one it lacks |
| `capabilityFlags.ts` | `CAPABILITY_FLAGS`, the capability bitflags as `gglib_core::domain::ModelCapabilities` numbers them, and `CapabilityFlagName` |
| `drawingValues.ts` | `drawingValuesFrom`, the drawing settings (default image model, MCP drawing) read from the stored `AppSettings` |

<!-- module-docs:end -->
