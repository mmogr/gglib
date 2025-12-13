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
