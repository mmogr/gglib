# paths

<!-- module-docs:start -->

Path utilities for gglib data directories and user-configurable locations.

This module provides the canonical path resolution for all gglib components:
- Database location
- Models directory
- Llama.cpp binaries
- Application data and resource roots

# Design

- Returns `PathBuf` and `PathError` for clear error handling
- No interactive/terminal I/O - adapters handle user prompts separately
- OS-specific logic is kept private in `platform`

<!-- module-docs:end -->
