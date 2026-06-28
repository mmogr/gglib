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
<!-- module-table:end -->

</details>
