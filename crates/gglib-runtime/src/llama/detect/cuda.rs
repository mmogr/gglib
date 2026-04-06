//! CUDA acceleration detection and GCC compatibility validation.
//!
//! This module handles all NVIDIA CUDA-related detection:
//!
//! - Checking whether the CUDA toolkit (`nvcc`) is installed
//! - Locating the CUDA installation path on disk
//! - Selecting the best host compiler for a CUDA build
//! - Validating CUDA / GCC version compatibility to prevent cryptic
//!   build failures
//!
//! # CUDA/GCC compatibility
//!
//! NVIDIA documents which GCC versions are supported as host compilers
//! for each CUDA toolkit release. Building with an unsupported GCC
//! version produces confusing template errors deep inside CUDA headers.
//! [`validate_cuda_gcc_compatibility`] catches this early and prints
//! actionable remediation steps.
//!
//! This validation is only performed on Linux where GCC/CUDA
//! compatibility issues exist. macOS uses Metal (no CUDA), and Windows
//! uses MSVC (a different compatibility matrix).

#[cfg(target_os = "linux")]
use anyhow::Context;
#[cfg(feature = "cli")]
use anyhow::Result;
#[cfg(target_os = "linux")]
use tracing::warn;

use super::tools::command_exists;
#[cfg(target_os = "linux")]
use super::tools::{command_stdout, parse_version_tuple};

// ============================================================================
// CUDA toolkit detection
// ============================================================================

/// Check if the system has the CUDA toolkit installed.
///
/// Returns `true` if `nvcc` is available in `$PATH`. This is the
/// definitive check — if `nvcc` isn't reachable the build will fail
/// regardless.
pub fn has_cuda_toolkit() -> bool {
    command_exists("nvcc")
}

/// Get the CUDA version as a `(major, minor)` tuple.
#[cfg(target_os = "linux")]
pub fn get_cuda_version_tuple() -> Option<(u32, u32)> {
    let stdout = command_stdout("nvcc", &["--version"])?;
    // Parse "Cuda compilation tools, release X.Y, ..." format
    for line in stdout.lines() {
        if line.contains("release")
            && let Some(pos) = line.find("release ")
        {
            let version_part = &line[pos + 8..];
            if let Some(comma_pos) = version_part.find(',') {
                return parse_version_tuple(&version_part[..comma_pos]);
            }
            return parse_version_tuple(version_part.split_whitespace().next()?);
        }
    }
    None
}

/// Get GCC version as a `(major, minor)` tuple from the default `gcc`.
#[cfg(target_os = "linux")]
fn get_gcc_version_tuple() -> Option<(u32, u32)> {
    let stdout = command_stdout("gcc", &["--version"])?;
    let first_line = stdout.lines().next()?;
    let version_str = first_line.split_whitespace().last()?;
    parse_version_tuple(version_str)
}

// ============================================================================
// CUDA path resolution
// ============================================================================

/// Get the CUDA installation path if available.
///
/// Searches in order:
/// 1. `$CUDA_PATH` environment variable
/// 2. `/opt/cuda` (Arch Linux / package manager)
/// 3. `/usr/local/cuda-*` (versioned installs, newest first)
/// 4. `/usr/local/cuda` (generic symlink)
/// 5. Windows standard locations
#[cfg(feature = "cli")]
pub fn get_cuda_path() -> Option<String> {
    if let Ok(cuda_path) = std::env::var("CUDA_PATH")
        && std::path::Path::new(&cuda_path).exists()
    {
        return Some(cuda_path);
    }

    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/opt/cuda").exists() {
            return Some("/opt/cuda".to_string());
        }

        if let Ok(entries) = std::fs::read_dir("/usr/local") {
            let mut cuda_dirs: Vec<_> = entries
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with("cuda-"))
                .collect();
            cuda_dirs.sort_by_key(|b| std::cmp::Reverse(b.file_name()));
            if let Some(newest) = cuda_dirs.first() {
                return Some(newest.path().to_string_lossy().to_string());
            }
        }

        if std::path::Path::new("/usr/local/cuda").exists() {
            return Some("/usr/local/cuda".to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(entries) =
            std::fs::read_dir("C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA")
        {
            let mut cuda_dirs: Vec<_> = entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .collect();
            cuda_dirs.sort_by_key(|b| std::cmp::Reverse(b.file_name()));
            if let Some(newest) = cuda_dirs.first() {
                return Some(newest.path().to_string_lossy().to_string());
            }
        }
    }

    None
}

// ============================================================================
// Compiler selection for CUDA builds
// ============================================================================

/// Detect which compiler will be used for CUDA builds.
///
/// Returns the compiler command and optionally its `(major, minor)` version
/// tuple. For Clang, the version is `None` because Clang has different
/// compatibility characteristics and is generally compatible with all
/// CUDA versions.
///
/// Selection priority:
/// 1. `$CC` environment variable (user override)
/// 2. `clang` (best overall compatibility)
/// 3. `gcc-12` / `gcc-11` (known CUDA-compatible versions)
/// 4. System `gcc` (fallback)
#[cfg(target_os = "linux")]
pub fn select_cuda_compiler_for_build() -> Result<(String, Option<(u32, u32)>)> {
    // Check user override
    if let Ok(cc) = std::env::var("CC") {
        if cc.contains("clang") {
            return Ok(("clang".to_string(), None));
        } else if cc.starts_with("gcc") || cc == "gcc" {
            let version_str = get_specific_gcc_version(&cc)?;
            let version = parse_version_tuple(&version_str);
            return Ok((cc, version));
        } else {
            warn!(
                compiler = %cc,
                "CC is set to a non-standard compiler, version validation will be skipped"
            );
            return Ok((cc, None));
        }
    }

    // Prefer clang (best CUDA compatibility, skip validation)
    if command_exists("clang") {
        return Ok(("clang".to_string(), None));
    }

    // Try known CUDA-compatible GCC versions
    for gcc_cmd in &["gcc-12", "gcc-11"] {
        if command_exists(gcc_cmd) {
            let version_str = get_specific_gcc_version(gcc_cmd)?;
            let version = parse_version_tuple(&version_str);
            return Ok((gcc_cmd.to_string(), version));
        }
    }

    // Fallback to system GCC
    let version = get_gcc_version_tuple();
    Ok(("gcc".to_string(), version))
}

/// Get the version string of a specific GCC binary.
#[cfg(target_os = "linux")]
fn get_specific_gcc_version(gcc_cmd: &str) -> Result<String> {
    let stdout = command_stdout(gcc_cmd, &["--version"])
        .with_context(|| format!("Failed to run {gcc_cmd}"))?;

    stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Failed to parse {gcc_cmd} version"))
}

// ============================================================================
// CUDA / GCC compatibility validation
// ============================================================================

/// Validate that the detected CUDA toolkit and host GCC are compatible.
///
/// Returns `Ok(())` if the combination is supported, or an `Err` with
/// a detailed message including remediation steps. Validation is only
/// performed on Linux; other platforms return `Ok(())` unconditionally.
#[cfg(feature = "cli")]
pub fn validate_cuda_gcc_compatibility() -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let (cuda_major, cuda_minor) = match get_cuda_version_tuple() {
            Some(v) => v,
            None => {
                anyhow::bail!(
                    "CUDA toolkit not detected. Please install CUDA or build without --cuda flag."
                );
            }
        };

        let (compiler_name, gcc_version) = select_cuda_compiler_for_build()?;

        // Clang is generally compatible with all CUDA versions
        if compiler_name.contains("clang") {
            return Ok(());
        }

        let (gcc_major, _gcc_minor) = match gcc_version {
            Some(v) => v,
            None => return Ok(()), // Can't determine version, let the build try
        };

        // CUDA/GCC compatibility matrix
        // Source: https://docs.nvidia.com/cuda/cuda-installation-guide-linux/
        let is_compatible = match cuda_major {
            13 => gcc_major <= 12,
            12 => gcc_major <= 13,
            11 => gcc_major <= 11,
            _ => true,
        };

        if !is_compatible {
            let mut msg = format!(
                "\n\n❌ CUDA/GCC Compatibility Issue:\n\n\
                 CUDA {cuda_major}.{cuda_minor} does not support \
                 {compiler_name} {gcc_major}.x as a host compiler.\n\n\
                 Supported workarounds:\n\n"
            );

            match cuda_major {
                13 => msg.push_str(
                    "  1. Install GCC 12 (recommended for CUDA 13.x):\n\n\
                     \x20    sudo apt install gcc-12 g++-12\n\
                     \x20    export CC=gcc-12\n\
                     \x20    export CXX=g++-12\n\
                     \x20    gglib config llama install --cuda\n\n\
                     \x20 2. Upgrade to CUDA 12.x (supports GCC 13.x):\n\n\
                     \x20    https://developer.nvidia.com/cuda-downloads\n\n\
                     \x20 3. Use Clang (if compatible with your CUDA version):\n\n\
                     \x20    sudo apt install clang\n\
                     \x20    gglib config llama install --cuda\n",
                ),
                12 => msg.push_str(
                    "  1. Install GCC 13 or earlier:\n\n\
                     \x20    sudo apt install gcc-13 g++-13\n\
                     \x20    export CC=gcc-13\n\
                     \x20    export CXX=g++-13\n\
                     \x20    gglib config llama install --cuda\n",
                ),
                11 => msg.push_str(
                    "  1. Install GCC 11 (required for CUDA 11.x):\n\n\
                     \x20    sudo apt install gcc-11 g++-11\n\
                     \x20    export CC=gcc-11\n\
                     \x20    export CXX=g++-11\n\
                     \x20    gglib config llama install --cuda\n\n\
                     \x20 2. Upgrade to CUDA 12.x or newer (recommended)\n",
                ),
                _ => msg.push_str(
                    "  1. Check CUDA documentation for supported GCC versions\n\
                     \x20 2. Install a compatible GCC version\n",
                ),
            }

            msg.push_str(
                "\nFor more information, see:\n\
                 https://docs.nvidia.com/cuda/cuda-installation-guide-linux/\n\n",
            );

            anyhow::bail!(msg);
        }

        Ok(())
    }
}
