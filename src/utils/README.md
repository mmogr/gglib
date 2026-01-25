<!-- module-docs:start -->

# Utilities Module

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-utils-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-utils-complexity.json)

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
- **`gguf_parser.rs`**: Specialized parser for extracting metadata from GGUF files. Includes `detect_reasoning_support()` for auto-detecting reasoning models (DeepSeek R1, Qwen3, QwQ, etc.) from chat template patterns and model names.
- **`input.rs`**: CLI user input handling (prompts, confirmations).
- **`process/`**: Low-level process management utilities.
  - **`log_streamer.rs`**: `ServerLogManager` for capturing and broadcasting llama-server stdout/stderr logs. Provides real-time log streaming via broadcast channels and maintains a ring buffer of recent logs per server port.
  - **`events.rs`**: `ServerEvent` types for lifecycle state synchronization. Defines the event schema used by both Tauri and SSE to notify frontends of server state changes.
  - **`event_broadcaster.rs`**: `ServerEventBroadcaster` for SSE clients in web mode. Uses a tokio broadcast channel to fan out events to multiple connected clients.

<!-- module-docs:end -->
