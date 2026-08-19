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
| `dashboard.ts` | `SlotSnapshot`, `ActiveConnectionSnapshot`, `SamplingAuditSnapshot`, `DashboardSnapshot` | Every field of every frame, since nothing on the dashboard contract skips. The whole-snapshot builder replaces two tests that named five of sixteen fields behind a cast. Values are the ones the proxy can actually emit — `slots_status` is the poller's own default, not the GUI's fallback string. |

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
