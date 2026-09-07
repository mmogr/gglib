# LLM Completion

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-llm_completion-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-llm_completion-complexity.json)

<!-- module-docs:start -->

Concrete [`LlmCompletionPort`] adapter for a llama-server instance.

Translates domain [`AgentMessage`] / [`ToolDefinition`] values into the
OpenAI-compatible JSON wire format, POSTs to
`{base_url}/v1/chat/completions` with `"stream": true`, and maps the
response SSE frames back to [`LlmStreamEvent`] values.

The `base_url` is the server root without a trailing path component,
e.g. `"http://127.0.0.1:9000"`.  This allows the adapter to target any
reachable host (Docker networks, remote servers, CI environments).

# Authentication

The adapter sends nothing by default: a llama-server on loopback asks for
nothing. [`LlmCompletionAdapter::with_far_machine`] adds
`Authorization: Bearer …` to every request, for the one upstream that demands
it — the remote tunnel's loopback port, which is another machine's proxy
(ADR 0012) and whose listener deliberately injects no credential of its own.
The token is held in a field the struct never prints; neither it nor
[`FarMachine`] derives `Debug`.

The key does not travel alone: it arrives in a [`FarMachine`], beside the
fingerprint of the machine that issued it. Only the surface that resolved the
remote upstream knows whose port it just chose, and the adapter it hands the
result to holds a URL that is loopback either way — so if that identity is
ever to be usable down here, it has to come down with the key.

# Layout

`mod.rs` holds the struct and its request path; `builder.rs` the two
constructors and the `with_*` builders; `far_machine.rs` the other end of a
remote turn; `retry/` the send loop; `body.rs` and `stream.rs` the two ends
of the wire format.

# Lifetime

Prefer constructing one adapter **per request** via
[`LlmCompletionAdapter::with_client`] and passing a clone of the
application-level `reqwest::Client` (stored in `AppState`) so all requests
share a single connection pool.  The `new` constructor is still available
for standalone use (e.g. CLI) and allocates its own pool.

```ignore
let adapter = LlmCompletionAdapter::new("http://127.0.0.1:9000", None::<String>);
let agent   = AgentLoop::build(Arc::new(adapter), tool_executor, None);
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`body.rs`](body.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-body-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-body-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-body-coverage.json) |
| [`builder.rs`](builder.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-builder-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-builder-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-builder-coverage.json) |
| [`far_machine.rs`](far_machine.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-far_machine-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-far_machine-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-far_machine-coverage.json) |
| [`shaping_tests.rs`](shaping_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-shaping_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-shaping_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-shaping_tests-coverage.json) |
| [`stream.rs`](stream.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-stream-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-stream-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-stream-coverage.json) |
| [`retry/`](retry/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-coverage.json) |
<!-- module-table:end -->

</details>
