# install

<!-- module-docs:start -->

Source-build installation pipeline for llama.cpp.

A build is of the release the install selects, the pin unless
`GGLIB_LLAMA_RELEASE` names another, cloned from `ggml-org/llama.cpp` by the
checkout every product's source build shares (`binary_install::checkout`).
It links statically and passes llama.cpp the tag's build number and commit,
so the installed binaries stand alone and report the release they are.

The primary streaming entry point is [`run_llama_source_build`], which emits
[`BuildEvent`] values into a `Sender<BuildEvent>` channel. CLI surface concerns
(dependency checks, user prompts, progress rendering) live in `gglib-cli`, in
`handlers::config::llama_install` and `handlers::config::llama_events`.

## Consumer table

| Consumer | Crate        | Output                                                                       |
|----------|--------------|------------------------------------------------------------------------------|
| CLI      | `gglib-cli`  | `indicatif` spinner + progress bar in `handlers::config::llama_events`       |
| Axum     | `gglib-axum` | SSE stream at `POST /api/config/system/update-llama`, via `run_llama_update` |

There were two more. #834 removed both as dead end to end — the SSE route at
`POST /api/system/build-llama-from-source` and the Tauri command behind it,
neither of which had a caller — taking the `llama-build-progress` event the
`WebView` listened for with them. See `llama/build_events.rs`.

## Threading model

[`prepare_checkout`] and [`build_llama_cpp`] call `blocking_send` directly in their
function bodies and must run via [`tokio::task::spawn_blocking`] from async contexts.
[`run_llama_source_build`] handles this wrapping automatically.

<!-- module-docs:end -->
