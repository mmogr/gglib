# install

<!-- module-docs:start -->

Source-build installation pipeline for llama.cpp.

The primary streaming entry point is [`run_llama_source_build`], which emits
[`BuildEvent`] values into a `Sender<BuildEvent>` channel. CLI surface concerns
(dependency checks, user prompts, progress rendering) live in
`gglib-cli::handlers::llama_install`.

## Consumer table

| Consumer | Crate        | Output                                                          |
|----------|--------------|-----------------------------------------------------------------|
| CLI      | `gglib-cli`  | `indicatif` spinner + progress bar in `handlers::llama_install` |

There were two more. #834 removed both as dead end to end — the SSE route at
`POST /api/system/build-llama-from-source` and the Tauri command behind it,
neither of which had a caller — taking the `llama-build-progress` event the
WebView listened for with them. See `llama/build_events.rs`.

## Threading model

[`clone_llama_cpp`] and [`build_llama_cpp`] call `blocking_send` directly in their
function bodies and must run via [`tokio::task::spawn_blocking`] from async contexts.
[`run_llama_source_build`] handles this wrapping automatically.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
