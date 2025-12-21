# llama

llama.cpp management - installation, configuration, and server lifecycle.

## Purpose

This module handles all interactions with llama.cpp, including:
- Downloading and installing llama.cpp binaries
- Version management and updates
- Configuration and argument building
- Server availability checking
- Dependency validation
- Uninstallation

## Architecture Pattern

**Lifecycle Management**

```text
┌─────────────────────────────────────────────────────────────┐
│                    llama.cpp Lifecycle                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Install → Configure → Validate → Run → Monitor → Update   │
│     ▲                                        │              │
│     │                                        ▼              │
│     └──────────── Uninstall ◄──────────  Error             │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Module Organization

### Installation & Management
- **`ensure.rs`** - Ensure llama.cpp is installed and ready
  - Check if installed
  - Auto-download if missing
  - Verify installation integrity

- **`detect.rs`** - Detect existing llama.cpp installations
  - Search system PATH
  - Find local installations
  - Version detection

- **`deps.rs`** - Dependency checking and validation
  - Check for required system libraries
  - GPU driver validation
  - Platform-specific requirements

- **`update.rs`** - Update llama.cpp to newer versions
  - Check for updates
  - Download new versions
  - Migration and cleanup

- **`uninstall.rs`** - Clean removal of llama.cpp
  - Remove binaries
  - Clean up configuration
  - Remove temporary files

### Configuration
- **`config.rs`** - llama.cpp configuration management
  - Server configuration
  - Model-specific settings
  - Performance tuning

- **`args/`** - Command-line argument building
  - `base.rs` - Base argument types
  - `builder.rs` - Fluent argument builder
  - `gpu.rs` - GPU-specific arguments
  - `validation.rs` - Argument validation

### Runtime
- **`invocation.rs`** - Server process invocation
  - Build command lines
  - Environment setup
  - Process spawning

- **`server_availability.rs`** - Check if server is ready
  - Health endpoint polling
  - Startup detection
  - Timeout handling

- **`validate.rs`** - Installation and configuration validation
  - Binary validation
  - Configuration correctness
  - Compatibility checks

### User Interaction
- **`progress.rs`** - Installation progress tracking
  - Download progress
  - Installation steps
  - User feedback

- **`prompt.rs`** - User prompts and confirmations
  - Installation prompts
  - Update confirmations
  - Error recovery options

### Error Handling
- **`error.rs`** - llama.cpp-specific error types
  - Installation errors
  - Configuration errors
  - Runtime errors
  - Validation errors

## Usage Flow

### 1. Ensure Installation
```rust
use gglib_runtime::llama::ensure;

// Automatically installs if missing
ensure::ensure_llama_installed().await?;
```

### 2. Build Arguments
```rust
use gglib_runtime::llama::args::builder::ArgsBuilder;

let args = ArgsBuilder::new()
    .model_path("/path/to/model.gguf")
    .context_size(2048)
    .gpu_layers(35)
    .port(8080)
    .build()?;
```

### 3. Start Server
```rust
use gglib_runtime::llama::invocation;

let process = invocation::spawn_llama_server(&args).await?;
```

### 4. Check Availability
```rust
use gglib_runtime::llama::server_availability;

server_availability::wait_for_ready("http://localhost:8080", Duration::from_secs(30)).await?;
```

## Platform Support

- **macOS**: Native Metal GPU acceleration
- **Linux**: CUDA, ROCm, and Vulkan support
- **Windows**: CUDA and Vulkan support

Platform-specific behavior is handled through the `../system/` module.

## Dependencies

- **System probe**: `../system/` for GPU detection
- **Process management**: `../process/` for server lifecycle
- **Download**: `gglib-download` for binary downloads
- **Core types**: `gglib-core` for configuration types

## Testing

Tests focus on:
- Argument building and validation
- Configuration parsing
- Error handling
- Mock installation scenarios

Integration tests verify full installation flow on CI.

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`config.rs`](config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-config-coverage.json) |
| [`deps.rs`](deps.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-deps-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-deps-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-deps-coverage.json) |
| [`detect.rs`](detect.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-detect-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-detect-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-detect-coverage.json) |
| [`ensure.rs`](ensure.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-ensure-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-ensure-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-ensure-coverage.json) |
| [`error.rs`](error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-error-coverage.json) |
| [`invocation.rs`](invocation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-invocation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-invocation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-invocation-coverage.json) |
| [`progress.rs`](progress.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-progress-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-progress-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-progress-coverage.json) |
| [`prompt.rs`](prompt.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-prompt-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-prompt-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-prompt-coverage.json) |
| [`server_availability.rs`](server_availability.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-server_availability-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-server_availability-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-server_availability-coverage.json) |
| [`uninstall.rs`](uninstall.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-uninstall-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-uninstall-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-uninstall-coverage.json) |
| [`update.rs`](update.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-update-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-update-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-update-coverage.json) |
| [`validate.rs`](validate.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-validate-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-validate-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-validate-coverage.json) |
| [`args/`](args/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-coverage.json) |
| [`build/`](build/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-build-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-build-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-build-coverage.json) |
| [`download/`](download/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-download-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-download-coverage.json) |
| [`install/`](install/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-install-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-install-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-install-coverage.json) |
<!-- module-table:end -->
