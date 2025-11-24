<!-- module-docs:start -->

# Utilities Module

This module contains low-level helper functions and shared utilities used across the application.

## Architecture

```text
┌─────────────┐      ┌────────────────┐
│   System    │      │   Validation   │
│ (OS/Paths)  │      │ (Input/Files)  │
└──────┬──────┘      └───────┬────────┘
       │                     │
       ▼                     ▼
┌─────────────┐      ┌────────────────┐
│ GGUF Parser │      │    Process     │
│ (Metadata)  │      │ (Mgmt/Signals) │
└─────────────┘      └────────────────┘
```

## Components

- **`system.rs`**: OS-specific operations, hardware detection, and system info.
- **`paths.rs`**: Directory resolution for config, cache, and data storage.
- **`config.rs`**: Helpers for persisting `.env` overrides (models dir, fast download mode) and parsing runtime settings.
- **`validation.rs`**: Input validation and file integrity checks.
- **`gguf_parser.rs`**: Specialized parser for extracting metadata from GGUF files.
- **`input.rs`**: CLI user input handling (prompts, confirmations).
- **`process/`**: Low-level process management utilities.

<!-- module-docs:end -->
