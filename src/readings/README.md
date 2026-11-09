# readings

<!-- module-docs:start -->

Pure readers the contract tests pin: data in, a reading out, no React and no
IO. Each has a test under `tests/ts/contracts/` that holds it to the Rust
source or a checked-in contract file, which is why they live together and
apart from the components that show them.

| File | Purpose |
|------|---------|
| `contextReading.ts` | The conversation's context reading, from the figures of the reply that decides it (the newest, passing over an unfinished one without counts), and the sentences its detail says; nothing when a figure is missing. The rule of `contracts/context/readings.json` |
| `turnFigures.ts` | Which figures a reply shows: only those the page has for that turn, never a zero or a dash for one it lacks. The fold's summary (how long it thought, how many tool calls), the figure line (time, a computed rate, none for a turn of one token, tokens reused) and its tooltip's full set (model, quantisation, device, tokens read, reused and written), the model named only where it changes, and what a reply with no token yet waits for, with its prompt reading |
| `connect.ts` | What the Endpoint page says of a proof (answered, or why not) and of a client's recipes (key lines said as the stored secret): the words `render_proof` and `render_client_config` print, held to `contracts/proxy/` |
| `throughput.ts` | Tokens written per second of writing, for a request (Recent, the loaded inspector) and a reply (`turnFigures`): computed here, since no Rust does; nothing without both figures or with no time |
| `capabilityFlags.ts` | `CAPABILITY_FLAGS`, the capability bitflags as `gglib_core::domain::ModelCapabilities` numbers them, and `CapabilityFlagName` |
| `launchFit.ts` | The unload sentence from `plan.unloads`, the words `gglib model explain` prints, held to `contracts/models/unload_reason.json`; `tokens`, a context size as `32K`. The fit sentence is the daemon's, served on `plan.fit.sentence` |
| `drawingValues.ts` | `drawingValuesFrom`, the drawing settings (default image model, MCP drawing) read from the stored `AppSettings` |

<!-- module-docs:end -->
