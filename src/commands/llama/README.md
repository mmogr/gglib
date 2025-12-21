<!-- module-docs:start -->

# Llama Management Module

This module handles the installation, building, and management of the `llama.cpp` backend. It includes logic for hardware detection and compilation.

## Architecture

```text
┌─────────────┐      ┌────────────────┐      ┌──────────────────┐
│ Install Cmd │ ───► │  Hardware Det  │ ───► │   Build System   │
│             │      │ (CUDA/Metal)   │      │ (make/cmake)     │
└─────────────┘      └────────────────┘      └────────┬─────────┘
                                                      │
                                                      ▼
                                             ┌──────────────────┐
                                             │   llama-server   │
                                             │     Binary       │
                                             └──────────────────┘
```

## Components

- **install.rs**: Orchestrates the downloading and building of `llama.cpp`.
- **detect.rs**: Detects available hardware acceleration (CUDA, Metal, Vulkan, etc.).
- **build.rs**: Handles the actual compilation process.
- **deps.rs**: Checks for required system dependencies (git, cmake, compilers).
- **update.rs**: Handles updating the `llama.cpp` source code.
- **validate.rs**: Verifies the installation and build artifacts.

<!-- module-docs:end -->
