# build

<!-- module-docs:start -->

Build orchestration for llama.cpp.

[`build_llama_cpp`] emits [`BuildEvent`] values over a
`tokio::sync::mpsc::Sender<BuildEvent>` so that callers can render progress
without this module knowing anything about terminals, HTTP, or Tauri.

## I/O Model

All subprocess output is routed through the `tokio::sync::mpsc::Sender<BuildEvent>`
channel supplied by the caller. The build functions do not write to the terminal
directly; the caller is responsible for adapting the event stream to its preferred
output (CLI spinner, SSE frames, Tauri events, etc.).

## Threading model

[`spawn_lines`] reads a subprocess's stdout and stderr on one OS thread each
([`std::thread::spawn`]) and merges their lines into one channel. The configure
and compile steps drain it and call `tx.blocking_send()`, so they run on an OS
thread themselves, never on a Tokio worker.

## Compiler flags

`CXXFLAGS`/`CFLAGS` are merged (read-then-append) during both the cmake configure
and build phases to carry `-O1`, which works around a GCC 15.2.1 ICE (internal
compiler error) that fires during higher optimisation passes on `chat.cpp`.

Additionally, `-DLLAMA_ALL_WARNINGS=OFF` is passed at configure time. Upstream
llama.cpp enables `-Wmissing-noreturn` (among others) via `LLAMA_ALL_WARNINGS`,
which floods the build log with hundreds of jinja template warnings we can't fix.
Since this is upstream code, we disable the extra warning set entirely.

Any `CXXFLAGS`/`CFLAGS` already present in the caller's environment are preserved
(see [`merge_flags`]).

<!-- module-docs:end -->
