# check_deps

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-complexity.json)

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

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`display.rs`](display) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-display-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-display-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-display-coverage.json) |
| [`platform.rs`](platform) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-platform-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-platform-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-check_deps-platform-coverage.json) |
| [`instructions/`](instructions/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-coverage.json) |
<!-- module-table:end -->

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
  - `windows.rs` - Windows-specific commands (Chocolatey, Scoop)

### Platform Detection
- **`platform.rs`** - OS and architecture detection
  - Identifies current platform
  - Routes to appropriate instruction set

## Checked Dependencies

### Required
- **git** - For cloning llama.cpp repository
- **cmake** - For building native code
- **C++ compiler** - Platform-specific (gcc/clang/MSVC)
- **make/ninja** - Build system

### Optional (GPU Acceleration)
- **CUDA toolkit** - For NVIDIA GPU support
- **Metal** - For Apple Silicon GPU support (built-in)
- **Vulkan SDK** - For cross-platform GPU support
- **ROCm** - For AMD GPU support

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
use gglib_cli::handlers::check_deps;
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
