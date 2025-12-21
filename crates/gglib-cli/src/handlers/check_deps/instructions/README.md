# instructions

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-complexity.json)

<!-- module-docs:start -->

Platform-specific installation instructions for system dependencies.

## Purpose

This module provides user-friendly, copy-paste installation commands for missing system dependencies. It detects the user's platform and displays appropriate package manager commands (Homebrew, apt, dnf, Chocolatey, etc.).

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                  instructions Module                         │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  check_deps → detect_os → platform module → instructions    │
│               (platform)   (this)                            │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Flow:**
1. Parent `check_deps` module identifies missing dependencies
2. Platform detection determines OS and distro
3. Appropriate platform-specific module formats instructions
4. Instructions printed to terminal for user to copy/paste

## Components

### Entry Point
**Module:** `mod.rs`

**Key Function:**
```rust,ignore
pub fn print_installation_instructions(missing: &[&Dependency])
```

Routes to platform-specific instruction generators based on detected OS.

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`common.rs`](common) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-common-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-common-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-common-coverage.json) |
| [`linux.rs`](linux) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-linux-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-linux-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-linux-coverage.json) |
| [`macos.rs`](macos) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-macos-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-macos-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-macos-coverage.json) |
| [`windows.rs`](windows) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-windows-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-windows-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-instructions-windows-coverage.json) |
<!-- module-table:end -->

### Platform Modules

#### `common.rs`
Cross-platform dependency information and shared utilities.

**Contains:**
- Common dependency descriptions
- Package name mappings across platforms
- Shared formatting functions

**Example:**
```rust,ignore
pub fn get_package_name(dep: &str, os: Os) -> &str {
    match (dep, os) {
        ("cmake", Os::MacOS) => "cmake",
        ("cmake", Os::Linux) => "cmake",
        ("cmake", Os::Windows) => "cmake",
        _ => dep,
    }
}
```

#### `macos.rs`
macOS-specific instructions using Homebrew.

**Example Output:**
```text
Missing Dependencies - macOS Installation Instructions:
──────────────────────────────────────────────────────

Install Homebrew (if not already installed):
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

Install required dependencies:
  brew install cmake git

For CUDA support (NVIDIA GPUs):
  CUDA is not officially supported on macOS.
  Use Metal acceleration instead (built-in for Apple Silicon).
```

**Special Handling:**
- Homebrew installation instructions if not present
- Metal GPU acceleration (built-in on Apple Silicon)
- No CUDA support warnings

#### `linux.rs`
Linux distribution-specific instructions.

**Supported Package Managers:**
- **apt** (Ubuntu, Debian)
- **dnf** (Fedora, RHEL)
- **pacman** (Arch Linux)
- **zypper** (openSUSE)
- **generic** (for unknown distros)

**Example Output (Ubuntu):**
```text
Missing Dependencies - Ubuntu/Debian Installation Instructions:
──────────────────────────────────────────────────────────────

Install required dependencies:
  sudo apt update
  sudo apt install cmake git build-essential

For CUDA support (NVIDIA GPUs):
  # Install CUDA Toolkit from NVIDIA:
  wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-keyring_1.1-1_all.deb
  sudo dpkg -i cuda-keyring_1.1-1_all.deb
  sudo apt update
  sudo apt install cuda-toolkit-12-3
```

**Distribution Detection:**
- Checks `/etc/os-release`
- Falls back to generic instructions
- Provides distro-specific commands

#### `windows.rs`
Windows-specific instructions using Chocolatey and Scoop.

**Example Output:**
```text
Missing Dependencies - Windows Installation Instructions:
────────────────────────────────────────────────────────

Using Chocolatey (recommended):
  # Install Chocolatey (if not already installed):
  Set-ExecutionPolicy Bypass -Scope Process -Force
  [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
  iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))

  # Install dependencies:
  choco install cmake git visualstudio2022-workload-vctools

Alternative - Using Scoop:
  # Install Scoop (if not already installed):
  Set-ExecutionPolicy RemoteSigned -Scope CurrentUser
  irm get.scoop.sh | iex

  # Install dependencies:
  scoop install cmake git mingw

For CUDA support (NVIDIA GPUs):
  Download and install CUDA Toolkit from:
  https://developer.nvidia.com/cuda-downloads?target_os=Windows
```

**Special Handling:**
- PowerShell-specific installation commands
- Both Chocolatey and Scoop options
- Visual Studio Build Tools for C++ compiler
- CUDA Toolkit download links

## Output Format

All platform modules follow a consistent format:

```text
Missing Dependencies - [Platform] Installation Instructions:
[separator line]

[Platform-specific package manager setup if needed]

Install required dependencies:
  [copy-paste commands]

[Optional GPU acceleration instructions]
```

## Usage Pattern

```rust,ignore
use crate::handlers::check_deps::instructions::print_installation_instructions;
use gglib_core::utils::system::Dependency;

// After checking dependencies
let missing: Vec<&Dependency> = dependencies
    .iter()
    .filter(|d| !d.is_installed())
    .collect();

if !missing.is_empty() {
    println!("\n⚠️  Some dependencies are missing.\n");
    print_installation_instructions(&missing);
}
```

## Design Notes

1. **Copy-Paste Ready** - All commands are immediately executable
2. **Context-Aware** - Instructions adapt to detected platform/distro
3. **Complete** - Includes package manager setup if needed
4. **GPU Support** - Provides optional CUDA/Metal/ROCm guidance
5. **Fallback** - Generic instructions for unknown platforms

## Testing

Tests focus on:
- Correct platform detection
- Appropriate command generation
- Package name mappings
- Output formatting

```rust,ignore
#[test]
fn test_macos_instructions() {
    let deps = vec![
        Dependency { name: "cmake", .. },
        Dependency { name: "git", .. },
    ];
    
    // Would capture output and verify format
    macos::print_instructions(&deps);
}
```

## Future Enhancements

- **nix** package manager support
- **snap** package support for Linux
- **winget** support for Windows 11
- Automatic clipboard copy of commands
- Interactive installation (run commands directly)
- Version-specific instructions (e.g., CUDA 12 vs 11)

<!-- module-docs:end -->
