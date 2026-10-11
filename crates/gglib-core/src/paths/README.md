# paths

<!-- module-docs:start -->

Path utilities for gglib data directories and user-configurable locations.

This module provides the canonical path resolution for all gglib components:
- Database location
- Models directory
- Llama.cpp binaries (`.llama/`)
- stable-diffusion.cpp's `sd-server` (`.sd/`)
- Application data and resource roots

# One data root for every build

Every gglib build keeps its state in one per-user folder: `~/.local/share/gglib`
on Linux, `~/Library/Application Support/gglib` on macOS, `%LOCALAPPDATA%\gglib`
on Windows. That holds the database, the `.env` overrides file, the daemon's
token and lock, pidfiles, logs, device keys, and the llama.cpp and sd-server
installs. A packaged install and a build from a checkout share it, so the
desktop app, the CLI and the daemon agree whichever binary runs, and moving
or cleaning a checkout changes nothing.

`GGLIB_DATA_DIR` names another folder, and `GGLIB_RESOURCE_DIR` moves the
llama.cpp and sd-server installs alone. A build with a folder of its own also
wants a daemon of its own, on the port `GGLIB_DAEMON_PORT` names.

`source_checkout` says whether this binary was built from a checkout that is
still there. It decides nothing about data: only whether llama.cpp is built
from source too, and where `gglib gui` looks for an app built beside it.

The database records a schema generation, and a build refuses a file a newer
build has written (`gglib-db`'s `setup_generation.rs`), so sharing one library
across versions fails loudly rather than corrupting it.

# The models directory

`set_models_dir` is the one way a surface changes it: it resolves the path,
creates the directory and stores it in the data root's `.env`, quoted, so
that a path with a space in it is still a line that file's loader reads.
`resolve_models_dir` reads it back: an explicit path first, then
`GGLIB_MODELS_DIR` in the environment, then the stored directory, read with
the loader's own parser, then the default. A daemon's download manager asks
it as each download starts and keeps the answer for that download's files
alone, so a directory stored while the daemon runs is where its next download
goes, unless the daemon's own environment names one.

# Design

- Returns `PathBuf` and `PathError` for clear error handling
- No interactive/terminal I/O - adapters handle user prompts separately
- OS-specific logic is kept private in `platform`

<!-- module-docs:end -->
