# Check Dependencies

<!-- module-docs:start -->

System dependency checking module for the CLI.

## Purpose

This module handles the `check-deps` command, which verifies that all required system dependencies for gglib are installed and properly configured. It provides platform-specific installation instructions when dependencies are missing.

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                    check_deps Module                         │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  User Command → Handler → Probe → Display → Instructions    │
│    (execute)    (mod.rs)  (core)  (display) (instructions)  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

## Components

### Core Handler
- **`mod.rs`** - Main execution logic
  - Coordinates dependency checking
  - Formats output table
  - Determines exit code based on required dependencies

### Display Layer
- **`display.rs`** - Output formatting
  - `print_dependency()` - Formats individual dependency status
  - `print_gpu_status()` - Shows GPU acceleration information
  - Color-coded status indicators (green = OK, red = missing)

### Installation Instructions
- **`instructions/`** - Platform-specific guidance
  - `common.rs` - Cross-platform instructions
  - `macos.rs` - macOS-specific commands (Homebrew)
  - `linux.rs` - Linux-specific commands (apt, dnf, pacman)
  - `windows.rs` - Windows-specific commands (winget, Chocolatey)

### Platform Detection
- **`platform.rs`** - OS and architecture detection
  - Identifies current platform
  - Routes to appropriate instruction set

## Checked Dependencies

The list is `DefaultSystemProbe::check_all_dependencies`, in `gglib-runtime`'s
`system/` module.

### Required
- **cargo**, **rustc** - The Rust toolchain
- **node**, **npm** - For building the web UI and the Tauri app
- **git** - For llama.cpp installation
- **make**, **cmake** - For the llama.cpp build
- **gcc**, **g++** - For llama.cpp compilation
- **pkg-config** - For building with system libraries
- **libssl-dev** - For llama.cpp's HTTPS support
- **Linux only:** patchelf, webkit2gtk-4.1, librsvg, libappindicator-gtk3,
  libasound2-dev, libcurl-dev, libsqlite3-dev, libclang-dev (the Tauri app,
  audio, llama.cpp's HTTP support, the database and bindgen)

### Optional
- **python3** - Enables the `hf_xet` download accelerator
- **gtk-layer-shell** (Linux) - Anchors the tray panel beside the system tray on Wayland

### GPU Acceleration
- **Metal** - Listed as available when detected (built-in on Apple GPUs)
- **CUDA** - Listed with its version when installed, or as optional when an NVIDIA GPU is detected without it
- **Vulkan runtime** - Listed when present, and then **Vulkan headers**, **glslc** and **SPIR-V headers** are required, since the build picks Vulkan
- **Vulkan** - Suggested as optional when neither Metal nor a Vulkan runtime is found (not on macOS)

## Output Format

```text
Checking system dependencies...

DEPENDENCY           STATUS          NOTES
=====================================================================================
git                  ✓ Installed     version 2.39.0
cmake                ✓ Installed     version 3.28.1
C++ Compiler         ✓ Installed     clang 15.0.0
make                 ✓ Installed     GNU Make 4.3
CUDA Toolkit         ✗ Missing       Optional - for NVIDIA GPU acceleration
Metal                ✓ Available     Apple Silicon GPU detected

GPU Acceleration: Metal available
All required dependencies are installed!
```

## Error Handling

The command returns:
- **Exit code 0** - All required dependencies present
- **Exit code 1** - One or more required dependencies missing

Missing dependencies trigger platform-specific installation instructions.

## Usage Pattern

```rust,ignore
use crate::handlers::check_deps;
use gglib_core::ports::SystemProbePort;

pub async fn handle_check_deps_command(
    probe: &dyn SystemProbePort
) -> Result<()> {
    check_deps::execute(probe).await
}
```

## Testing

Tests focus on:
- Dependency detection accuracy
- Installation instruction correctness
- Platform identification
- Output formatting

Mock `SystemProbePort` for unit testing:
```rust,ignore
#[tokio::test]
async fn test_all_deps_installed() {
    let mut mock_probe = MockSystemProbe::new();
    mock_probe.expect_check_all_dependencies()
        .returning(|| vec![
            Dependency {
                name: "git",
                status: DependencyStatus::Installed,
                ..Default::default()
            },
        ]);
    
    let result = check_deps::execute(&mock_probe).await;
    assert!(result.is_ok());
}
```

## Design Notes

1. **Separation of Concerns** - Display logic separated from detection logic
2. **Platform Abstraction** - Instructions module handles platform differences
3. **User-Friendly Output** - Color-coded, table-formatted results
4. **Actionable Guidance** - Provides copy-paste installation commands

<!-- module-docs:end -->
