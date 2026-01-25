<!-- module-docs:start -->

# Models Module

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-models-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-models-complexity.json)

This module defines the core data structures, database entities, and Data Transfer Objects (DTOs) used throughout the application.

## Architecture

```text
┌─────────────┐      ┌────────────────┐
│  Database   │ ───► │   Core Model   │
│   Entity    │      │    Structs     │
└──────┬──────┘      └───────┬────────┘
       │                     │
       ▼                     ▼
┌─────────────┐      ┌────────────────┐
│     DTOs    │ ◄─── │    Metadata    │
│ (API/JSON)  │      │   (GGUF Info)  │
└─────────────┘      └────────────────┘
```

## Components

- **`lib.rs` / `mod.rs`**: Core struct definitions (`Model`, `ModelFile`, etc.).
- **`gui.rs`**: DTOs specifically designed for the GUI/Web API responses.
- **Metadata**: Structures representing the GGUF file header and metadata key-value pairs.

<!-- module-docs:end -->
