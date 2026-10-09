# paths

<!-- module-docs:start -->

Path utilities for gglib data directories and user-configurable locations.

This module provides the canonical path resolution for all gglib components:
- Database location
- Models directory
- Llama.cpp binaries (`.llama/`)
- stable-diffusion.cpp's `sd-server` (`.sd/`)
- Application data and resource roots

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
