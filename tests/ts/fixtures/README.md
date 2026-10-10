# fixtures

Canonical responses, built the way the endpoint actually builds them.

A fixture here is not a convenience. It is the answer to a question the type
alone cannot settle: *what does the wire send when nothing is configured?* The
generated types say which keys exist and whether each may be `null`; they say
nothing about which of those states a real response is in. Every fixture in
this directory exists because a test was asserting against a response no
endpoint produces, and passing.

| File | Builds | The thing it gets right |
|------|--------|-------------------------|
| `inference.ts` | `InferenceConfig` | All eighteen keys, `null` for unset. `gglib-core`'s struct has no `skip_serializing_if`, so a two-field config is not a smaller response — it is not a response. |
| `settings.ts` | `AppSettings` | All twenty-three, `null` for unset. Same reason, on `gglib_app_services::types::AppSettings` — the type the endpoint returns, not the persisted `gglib_core::Settings`. |
| `model.ts` | `GuiModel` | The eight fields the old mirror made optional are required; only the three MoE fields, which really do carry `skip_serializing_if`, are omitted. |
| `explain.ts` | `SamplingExplanation` | `published` is an empty array rather than an absent key, and `defaultsOrigin` and `profile` are required nullables. `effortSuppressed` stays optional — it is the one field that genuinely skips. |
| `mcp.ts` | `McpServerInfo` | The nested `{server, status, tools}` every server route answers with, not the bare row two mocks were returning. |
| `ports.ts` | Test port constants | Centralised so a port is never hardcoded into a test, and CI can move them in one place. |
| `dashboard.ts` | `SlotSnapshot`, `ActiveConnectionSnapshot`, `ModelDefectCounts`, `SamplingAuditSnapshot`, `DashboardSnapshot` | Every field of every frame, since nothing on the dashboard contract skips. The whole-snapshot builder replaces two tests that named five of sixteen fields behind a cast. Values are the ones the proxy can actually emit — `slots_status` is the poller's own default, not the GUI's fallback string. |
| `downloads.ts` | `QueueSnapshot`, `DownloadRow` | The queue as `GET /api/models/downloads/queue` and a `queue_snapshot` event both carry it. An unknown value is an absent key, since every `Option` on the row skips; a waiting row with a known size has `percent: 0` and an empty `text.percent`; the idle queue has no `active` key. The words are the ones `row.rs` makes. |
| `agentic.ts` | `ArmScores`, `ArmDelta`, `TuneTaskResult`, `AgenticEvalReport` | Every key of each, since none of these structs skips a field: an arm that did not run is `null`, an empty list is `[]`, and a field with a serde default carries that default. |
| `fakeFarDaemon.ts` | This machine's daemon with a far machine joined, behind `fetch` | `/api/remote/*` answered as the daemon forwards it, by a second `FakeDaemon` as the far machine: chats listed with each one's `live_run`, opened with their rows, a turn of `{content, images?}` started and saved there, its images at `/api/remote/attachments` in the far machine's store, and `noImages` a far gglib from before images (`404` there, `400 invalid_request` for a turn naming one); every other path is this machine's. The far models as `/api/remote/models*` answers them: `PairedModels` with the paired row of the actions table and every entry, variants included; a `ModelLookup` with no `filePath` or `port`; a missing id as `404 model_not_found`; a load, after which the model is serving. What the page sent to the far routes is recorded as sent. |
| `fakeDaemon.ts` | The chat and runs routes behind `fetch` | Bodies as the routes send them: bare numbers from `POST /api/conversations` and `DELETE /api/messages/{id}`; a run `queued` at `201`, a repeated id `200`; the user's row saved only once a run is accepted, and none by a run that answers the question the chat ends in (`answer_saved`, refused when it ends in none); `/thread` with the branch points of the chat's family and `answerable`, and `/changes` made as gglib's branching rules say (`fakeBranches.ts`), a branch's copies keyed by the rows they copy; a run's end saved after the call that asked, and only then read as ended; `/api/attachments` the image store, a run whose images it would refuse refused by code before it starts, and a saved user row listing its images. Held to `contracts/runs/recorded.json` by `tests/ts/contracts/fakeDaemonRuns.test.ts`. |
| `fakeBranches.ts` | gglib's branching rules, for `fakeDaemon.ts` | Which change is made in place, which on a new chat and which is refused, whether a chat ends in a question, and the branch points a family holds along a chat with each option's line, answered as `crates/gglib-core/src/domain/branching` answers them. Held to `contracts/chats/branching.json` by `tests/ts/contracts/branching.test.ts`. |
| `fakeImageStore.ts` | A daemon's image store behind the fake daemons | `POST` takes the raw bytes, keeps a PNG or a JPEG under their SHA-256 and answers its facts and `image_tokens` as the store works them out; `GET /{id}` the bytes back as a page `Blob`; and the pre-run check: an id not stored, then over 16 MiB together, history included. `png` and `pngFile` build a PNG of a given size from its header alone. |

## Rules

**Default to "nothing configured".** The baseline each builder spreads over is
the fresh-install state, because that is what the code resolving its own
fallbacks has to be tested against. Pass overrides for the interesting fields
and let the rest stay null.

**Do not use these for request shapes.** A form's in-progress state and an
update body are legitimately sparse — a `SparseInferenceConfig`, a bare object
literal — and wrapping one of these in a request fixture would assert that the
client sends eighteen keys when it should send the two the user touched.

**Verify against Rust, not against the TypeScript.** The generated types are
themselves derived, so agreeing with them proves nothing about serde's
behaviour. `skip_serializing_if` on the field is the fact that decides whether
a key can be absent.
