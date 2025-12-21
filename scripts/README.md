# GGLib Helper Scripts

This directory contains helper scripts (Rust and shell) used for development and maintenance.

## Scripts

### `check-deps.sh`
Verifies that all necessary system dependencies are installed.
- Checks for: `cargo`, `npm`, `cmake`, `git`
- Used by `make check-deps`

### `install-llama.sh`
Automated script to download, build, and install `llama.cpp`.
- Detects OS (macOS, Linux)
- Detects Hardware (Apple Silicon, NVIDIA GPU)
- Configures CMake with appropriate flags (Metal, CUDA, etc.)
- Installs binaries to `.llama/bin/`

## Usage

These scripts are typically invoked via the `Makefile` in the root directory, but can be run manually:

```bash
./scripts/check-deps.sh
./scripts/install-llama.sh
```
