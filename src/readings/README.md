# readings

<!-- module-docs:start -->

Pure readers the contract tests pin: data in, a reading out, no React and no
IO. Each has a test under `tests/ts/contracts/` that holds it to the Rust
source or a checked-in contract file, which is why they live together and
apart from the components that show them.

| File | Purpose |
|------|---------|
| `contextReading.ts` | The conversation's context reading, from the figures of the reply that decides it (the newest, passing over an unfinished one without counts), and the sentences its detail says; nothing when a figure is missing. The rule of `contracts/context/readings.json` |
| `turnFigures.ts` | Which figures a reply shows: only those the page has for that turn, never a zero or a dash for one it lacks. The fold's summary (how long it thought, how many tool calls), the figure line (time, the daemon's rate, none for a turn of one token, tokens reused) and its tooltip's full set (model, quantisation, device, tokens read, reused and written), the model named only where it changes, and what a reply with no token yet waits for (a wait in the daemon's own words, its frame's `said`), with its prompt reading |
| `connect.ts` | What the Endpoint page says of a proof (answered, or why not) and of a client's recipes (key lines said as the stored secret): the words `render_proof` and `render_client_config` print, held to `contracts/proxy/` |
| `rate.ts` | How fast a model wrote, in the words `rate_words` prints ("34 t/s"): the daemon's `tokens_per_second` for a request (Recent, the loaded inspector, beside a log) and a reply (`turnFigures`), and the newest a model has; nothing is divided here. Held to `contracts/proxy/figures.json` and `contracts/runs/turn_made.json` |
| `capabilityFlags.ts` | `CAPABILITY_FLAGS`, the capability bitflags as `gglib_core::domain::ModelCapabilities` numbers them, and `CapabilityFlagName` |
| `launchFit.ts` | `tokens`, a context size as `32K`. The fit and unload sentences are the daemon's, served on `plan.fit.sentence` and `plan.unloads.sentence` |
| `drawingValues.ts` | `drawingValuesFrom`, the drawing settings (default image model, MCP drawing) read from the stored `AppSettings` |

<!-- module-docs:end -->
