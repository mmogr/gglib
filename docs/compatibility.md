# What a version promises

From 1.0.0, gglib follows a simple rule. A release that breaks anything in the first list below is a new major version. There's no shim, alias or fallback kept for the old form. Minor and patch releases don't break them. Until 1.0.0, any release may still break them.

## Covered

- **The OpenAI routes:** `/v1/models`, `/v1/models/{name}`, `/v1/chat/completions`, `/v1/embeddings` and `/v1/images/generations`, as a client reads them.
- **gglib's own routes under `/v1/gglib/`**, which paired devices use, ggchat among them.
- **Error codes** gglib writes itself, as listed in [`error-codes.json`](error-codes.json).
- **CLI command and flag names**, as in the [CLI reference](cli/).
- **Settings keys.**
- **Your library.** A newer gglib opens a library an older one wrote, and upgrades it in place. An older gglib refuses a library a newer one has upgraded, rather than writing to it.

## Adding to an answer

A gglib answer only gains fields, and each new one is optional. A reader ignores any field it doesn't know, and any feature name in `/v1/models`' `gglib.features` it doesn't know. What a device sends to gglib's own routes is strict the other way: a hub refuses a turn on one of its chats, a change to a chat, or a branch whose body has a key it doesn't know. So a client sends a new key only to a hub whose `features` names the feature that brought it. Removing or changing a field is a new major version.

## Not covered

These can change in any release:

- the desktop app's and web page's look;
- `/api`, the management API the app and web page use;
- the CLI's human-readable output (its wording, tables and colours);
- benchmarks and diagnostics, including `model explain` and the proxy dashboard;
- the `GGLIB_DISABLE_*` switches;
- the Rust crates' APIs;
- the `llama_base_port` setting, which may go if llama-server's own router takes over residency;
- the loop guard: its `loop-guard` setting, and what it catches and does.

## Platforms

- **macOS (Apple silicon) and Linux:** built and tested.
- **Windows:** built and published, but not tested.
- **macOS builds aren't signed or notarized yet.** [Install](../README.md#pre-built-binaries-macos-linux-windows) says how to open them.
