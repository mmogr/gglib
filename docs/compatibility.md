# What a version promises

From 1.0.0, gglib follows a simple rule. A release that breaks anything in the first list below is a new major version. There's no shim, alias or fallback kept for the old form. Minor and patch releases don't break them. Until 1.0.0, any release may still break them.

## Covered

- **The OpenAI routes:** `/v1/models`, `/v1/models/{name}`, `/v1/chat/completions`, `/v1/embeddings` and `/v1/images/generations`, as a client reads them.
- **gglib's own routes under `/v1/gglib/`**, which paired devices use, ggchat among them.
- **Error codes** gglib writes itself, as listed in [`error-codes.json`](error-codes.json), and the object the proxy refuses with, `{"error": {"message", "type", "code"}}`.
- **CLI command and flag names, and settings keys:** the ones listed in [`promised_surface.txt`](../crates/gglib-cli/tests/promised_surface.txt), except lines marked `(not covered)` or `(hidden)`. The [CLI reference](cli/) says what each command does.
- **Your library.** A newer gglib opens a library an older one wrote, and upgrades it in place. An older gglib refuses a library a newer one has upgraded, rather than writing to it. When the older one is the `gglib` you typed and a copy on `PATH` opens the library, it runs your command with that copy and says so; on Windows, and for a build run from its own folder, it names the copy to run. After an upgrade, gglib names any copy on `PATH` that no longer opens the library. A library first made by gglib 0.7.5 or earlier isn't covered: gglib 0.35.0 and later can't open it. Move its `gglib.db` aside (`gglib config paths` shows where it is) and add your models to the new library gglib makes. Nor is a library gglib 0.37 or earlier wrote on Windows that holds a model or a chat folder: those builds stored each such path in Windows' long form (`\\?\C:\...`), later builds store the plain one (`C:\...`), and the library isn't converted. gglib refuses it, names the file, and says to move it aside, with `gglib.db-wal` and `gglib.db-shm` if they're beside it, since a new library made next to an old one's write-ahead log can be corrupted by it. gglib then makes a new library, without the old one's chats, settings, tool servers, or benchmark, compare and tune history, and `gglib model add` adds your models to it. The model files aren't touched. A Windows library with neither, and a library made on macOS or Linux, is upgraded as usual.

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
- the loop guard: its `loop-guard` setting, its log (`gglib proxy trips`), and what it catches and does.

## Platforms

- **macOS (Apple silicon) and Linux:** built and tested.
- **Drawing images on Linux:** the image runtime's pre-built release needs glibc 2.43 or newer, which is Ubuntu 26.04 and later. On an older system, Ubuntu 24.04 and Debian 13 among them, gglib builds it from source instead of downloading it. Chat runs on the same systems as before.
- **Windows 11 (x64):** built and tested with no GPU, from the installer to a working chat. A GPU, drawing images and pairing a second device aren't tested there yet. [#1959](https://github.com/mmogr/gglib/issues/1959) lists what was found and fixed.
- **macOS builds aren't signed or notarized yet.** [Install](../README.md#pre-built-binaries-macos-linux-windows) says how to open them.
