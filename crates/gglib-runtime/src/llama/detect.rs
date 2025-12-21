#![allow(dead_code)] // Utility functions may not all be used yet

//! Hardware acceleration detection for llama.cpp builds.

#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::Result;
use std::process::Command;
#[cfg(target_os = "linux")]
use tracing::warn;

// ============================================================================
// Version parsing utilities (inlined to avoid cross-crate dependency)
// ============================================================================

/// Parse a version string into a (major, minor) tuple.
///
/// Extracts only the leading numeric portion of each component,
/// so "12.0-rc1" successfully parses as (12, 0).
fn parse_version_tuple(version_str: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() >= 2 {
        let parse_numeric = |part: &str| -> Option<u32> {
            let numeric_str: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            numeric_str.parse::<u32>().ok()
        };
        let major = parse_numeric(parts[0])?;
        let minor = parse_numeric(parts[1])?;
        Some((major, minor))
    } else {
        None
    }
}

/// Get CUDA version as a tuple (major, minor).
#[cfg(target_os = "linux")]
fn get_cuda_version_tuple() -> Option<(u32, u32)> {
    let output = Command::new("nvcc").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse "Cuda compilation tools, release X.Y, ..." format
    for line in stdout.lines() {
        if line.contains("release") {
            // Find version after "release "
            if let Some(pos) = line.find("release ") {
                let version_part = &line[pos + 8..];
                if let Some(comma_pos) = version_part.find(',') {
                    return parse_version_tuple(&version_part[..comma_pos]);
                }
                return parse_version_tuple(version_part.split_whitespace().next()?);
            }
        }
    }
    None
}

/// Get GCC version as a tuple (major, minor).
#[cfg(target_os = "linux")]
fn get_gcc_version_tuple() -> Option<(u32, u32)> {
    let output = Command::new("gcc").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse first line, version is usually the last word
    let first_line = stdout.lines().next()?;
    let version_str = first_line.split_whitespace().last()?;
    parse_version_tuple(version_str)
}

// ============================================================================
// Acceleration types and detection
// ============================================================================

/// Acceleration type for llama.cpp build
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceleration {
    /// Metal acceleration (Apple Silicon)
    Metal,
    /// CUDA acceleration (NVIDIA)
    Cuda,
    /// CPU only (no acceleration)
    Cpu,
}

impl Acceleration {
    /// Get the display name for this acceleration type
    pub fn display_name(&self) -> &str {
        match self {
            Acceleration::Metal => "Metal",
            Acceleration::Cuda => "CUDA",
            Acceleration::Cpu => "CPU",
        }
    }

    /// Get the `CMake` flags for this acceleration type
    pub fn cmake_flags(&self) -> Vec<&str> {
        match self {
            Acceleration::Metal => vec!["-DGGML_METAL=ON"],
            Acceleration::Cuda => vec!["-DGGML_CUDA=ON"],
            Acceleration::Cpu => vec![],
        }
    }
}

impl std::fmt::Display for Acceleration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Detect the optimal acceleration type for the current system
pub fn detect_optimal_acceleration() -> Acceleration {
    // Priority: Metal (macOS) > CUDA (NVIDIA) > CPU
    if cfg!(target_os = "macos") && has_metal_support() {
        Acceleration::Metal
    } else if has_cuda_toolkit() {
        Acceleration::Cuda
    } else {
        Acceleration::Cpu
    }
}

/// Check if the system has Metal support (Apple Silicon or recent Intel Macs)
fn has_metal_support() -> bool {
    #[cfg(target_os = "macos")]
    {
        // Check for Apple Silicon
        if cfg!(target_arch = "aarch64") {
            return true;
        }

        // Check macOS version for Intel Macs (Metal requires 10.13+)
        if let Ok(output) = Command::new("sw_vers").arg("-productVersion").output()
            && let Ok(version) = String::from_utf8(output.stdout)
            && let Some(major) = version.split('.').next()
            && let Ok(major_num) = major.trim().parse::<u32>()
        {
            return major_num >= 10;
        }
    }

    false
}

/// Check if the system has CUDA toolkit installed
fn has_cuda_toolkit() -> bool {
    // Check if nvcc (CUDA compiler) is available in PATH
    // This is the definitive check - if nvcc isn't in PATH, the build will fail
    Command::new("nvcc").arg("--version").output().is_ok()
}

/// Get the CUDA installation path if available
pub fn get_cuda_path() -> Option<String> {
    // Check environment variable first (Windows/Linux standard)
    if let Ok(cuda_path) = std::env::var("CUDA_PATH")
        && std::path::Path::new(&cuda_path).exists()
    {
        return Some(cuda_path);
    }

    // Check common CUDA installation directories (Linux/Windows)
    #[cfg(target_os = "linux")]
    {
        // Check Arch Linux / package manager installation first
        if std::path::Path::new("/opt/cuda").exists() {
            return Some("/opt/cuda".to_string());
        }

        // Check for versioned installations first (most specific)
        if let Ok(entries) = std::fs::read_dir("/usr/local") {
            let mut cuda_dirs: Vec<_> = entries
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with("cuda-"))
                .collect();

            // Sort by version (newest first)
            cuda_dirs.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

            if let Some(newest) = cuda_dirs.first() {
                return Some(newest.path().to_string_lossy().to_string());
            }
        }

        // Check for generic /usr/local/cuda symlink
        if std::path::Path::new("/usr/local/cuda").exists() {
            return Some("/usr/local/cuda".to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Check standard Windows CUDA locations
        if let Ok(entries) =
            std::fs::read_dir("C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA")
        {
            let mut cuda_dirs: Vec<_> = entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .collect();

            // Sort by version (newest first)
            cuda_dirs.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

            if let Some(newest) = cuda_dirs.first() {
                return Some(newest.path().to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Check if git is installed
pub fn has_git() -> Result<Option<String>> {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version
                .trim()
                .strip_prefix("git version ")
                .unwrap_or(version.trim())
                .to_string();
            Ok(Some(version))
        }
        _ => Ok(None),
    }
}

/// Check if cmake is installed
pub fn has_cmake() -> Result<Option<String>> {
    match Command::new("cmake").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            // Extract version number from first line
            let version = version
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(2))
                .unwrap_or("unknown")
                .to_string();
            Ok(Some(version))
        }
        _ => Ok(None),
    }
}

/// Check if a C++ compiler is installed
pub fn has_cpp_compiler() -> Result<Option<String>> {
    // Try different compilers in order of preference
    let compilers = if cfg!(target_os = "windows") {
        vec!["cl", "g++", "clang++"]
    } else if cfg!(target_os = "macos") {
        vec!["clang++", "g++"]
    } else {
        vec!["g++", "clang++"]
    };

    for compiler in compilers {
        match Command::new(compiler).arg("--version").output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                let first_line = version.lines().next().unwrap_or("unknown");
                return Ok(Some(format!("{} ({})", compiler, first_line.trim())));
            }
            _ => continue,
        }
    }

    Ok(None)
}

/// Get the number of CPU cores for parallel compilation
pub fn get_num_cores() -> usize {
    num_cpus::get()
}

/// Detect which compiler will be used for CUDA builds
/// Returns the compiler command and optionally its version tuple
/// This implements the same selection priority as used in the build process
/// For clang, returns None as version since clang has different compatibility characteristics
#[cfg(target_os = "linux")]
pub fn select_cuda_compiler_for_build() -> Result<(String, Option<(u32, u32)>)> {
    // On Linux, check for specific GCC versions first (CUDA compatibility)
    #[cfg(target_os = "linux")]
    {
        // Check if user has set CC environment variable
        if let Ok(cc) = std::env::var("CC") {
            if cc.contains("clang") {
                // User explicitly set clang - skip version validation
                return Ok(("clang".to_string(), None));
            } else if cc.starts_with("gcc") || cc == "gcc" {
                // User set a specific GCC - try to get its version
                let version_str = get_specific_gcc_version(&cc)?;
                // If version parsing fails, return None - validation will be skipped
                let version = parse_version_tuple(&version_str);
                return Ok((cc, version));
            } else {
                // User set a custom compiler (e.g., icc) - respect it but skip validation
                warn!(
                    compiler = %cc,
                    "CC is set to a non-standard compiler, version validation will be skipped"
                );
                return Ok((cc, None));
            }
        }

        // Check for clang first (best CUDA compatibility)
        // Clang is generally compatible with all CUDA versions, so we skip validation
        if Command::new("clang").arg("--version").output().is_ok() {
            return Ok(("clang".to_string(), None));
        }

        // Check for gcc-12 (CUDA 13.x compatible)
        if Command::new("gcc-12").arg("--version").output().is_ok() {
            let version_str = get_specific_gcc_version("gcc-12")?;
            // If version parsing fails, return None - validation will be skipped
            let version = parse_version_tuple(&version_str);
            return Ok(("gcc-12".to_string(), version));
        }

        // Check for gcc-11 (CUDA 11.x compatible)
        if Command::new("gcc-11").arg("--version").output().is_ok() {
            let version_str = get_specific_gcc_version("gcc-11")?;
            // If version parsing fails, return None - validation will be skipped
            let version = parse_version_tuple(&version_str);
            return Ok(("gcc-11".to_string(), version));
        }
    }

    // On all platforms, check for clang (if not already checked on Linux)
    #[cfg(not(target_os = "linux"))]
    {
        if Command::new("clang").arg("--version").output().is_ok() {
            return Ok(("clang".to_string(), None));
        }
    }

    // Fall back to system GCC/compiler
    // If version can't be determined, return None - validation will be skipped
    let version = get_gcc_version_tuple();
    Ok(("gcc".to_string(), version))
}

/// Get version of a specific gcc binary
#[cfg(target_os = "linux")]
fn get_specific_gcc_version(gcc_cmd: &str) -> Result<String> {
    let output = Command::new(gcc_cmd)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to run {}", gcc_cmd))?;

    if !output.status.success() {
        anyhow::bail!("{} --version failed", gcc_cmd);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .ok_or_else(|| anyhow::anyhow!("Failed to parse {} version", gcc_cmd))?
        .to_string();

    Ok(version)
}

/// Validate CUDA and GCC compatibility
/// Returns Ok if compatible, Err with detailed message if not
/// This validation is only performed on Linux where GCC/CUDA compatibility issues exist
/// macOS uses Metal (no CUDA), Windows uses MSVC (different compatibility matrix)
pub fn validate_cuda_gcc_compatibility() -> Result<()> {
    // Only validate on Linux - macOS doesn't use CUDA, Windows uses MSVC
    #[cfg(not(target_os = "linux"))]
    {
        Ok(()) // Skip validation on non-Linux platforms
    }

    #[cfg(target_os = "linux")]
    {
        use get_cuda_version_tuple;

        let cuda_version = get_cuda_version_tuple();

        let (cuda_major, cuda_minor) = match cuda_version {
            Some(v) => v,
            None => {
                // CUDA not detected, but we're trying to build with CUDA?
                anyhow::bail!(
                    "CUDA toolkit not detected. Please install CUDA or build without --cuda flag."
                );
            }
        };

        // Detect which compiler will actually be used
        let (compiler_name, gcc_version) = select_cuda_compiler_for_build()?;

        // Skip validation for clang - it has different compatibility characteristics
        // and is generally compatible with all CUDA versions
        if compiler_name.contains("clang") {
            return Ok(());
        }

        // Get GCC version for validation (skip if we couldn't parse it)
        let (gcc_major, gcc_minor) = match gcc_version {
            Some(v) => v,
            None => {
                // Couldn't determine GCC version, let the build try and fail naturally
                return Ok(());
            }
        };

        // CUDA/GCC compatibility matrix
        // Source: https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html#host-compiler-support-policy
        let is_compatible = match cuda_major {
            13 => gcc_major <= 12, // CUDA 13.x supports GCC up to 12.x
            12 => gcc_major <= 13, // CUDA 12.x supports GCC up to 13.x
            11 => gcc_major <= 11, // CUDA 11.x supports GCC up to 11.x
            _ => true,             // Unknown CUDA version, let it try
        };

        if !is_compatible {
            let mut error_msg = format!(
                "\n\n❌ CUDA/GCC Compatibility Issue:\n\n\
             CUDA {}.{} does not support {} {}.{} as a host compiler.\n\n",
                cuda_major, cuda_minor, compiler_name, gcc_major, gcc_minor
            );

            // Provide specific recommendations based on CUDA version
            error_msg.push_str("Supported workarounds:\n\n");

            match cuda_major {
                13 => {
                    error_msg.push_str(
                        "  1. Install GCC 12 (recommended for CUDA 13.x):\n\
\n\
     sudo apt install gcc-12 g++-12\n\
     export CC=gcc-12\n\
     export CXX=g++-12\n\
     gglib llama install --cuda\n\
\n\
  2. Upgrade to CUDA 12.x (supports GCC 13.x):\n\
\n\
     https://developer.nvidia.com/cuda-downloads\n\
\n\
  3. Use Clang (if compatible with your CUDA version):\n\
\n\
     sudo apt install clang\n\
     gglib llama install --cuda\n",
                    );
                }
                12 => {
                    error_msg.push_str(
                        "  1. Install GCC 13 or earlier:\n\
\n\
     sudo apt install gcc-13 g++-13\n\
     export CC=gcc-13\n\
     export CXX=g++-13\n\
     gglib llama install --cuda\n",
                    );
                }
                11 => {
                    error_msg.push_str(
                        "  1. Install GCC 11 (required for CUDA 11.x):\n\
\n\
     sudo apt install gcc-11 g++-11\n\
     export CC=gcc-11\n\
     export CXX=g++-11\n\
     gglib llama install --cuda\n\
\n\
  2. Upgrade to CUDA 12.x or newer (recommended)\n",
                    );
                }
                _ => {
                    error_msg.push_str(
                        "  1. Check CUDA documentation for supported GCC versions\n\
  2. Install a compatible GCC version\n",
                    );
                }
            }

            error_msg.push_str("\nFor more information, see:\n");
            error_msg.push_str("https://docs.nvidia.com/cuda/cuda-installation-guide-linux/\n\n");

            anyhow::bail!(error_msg);
        }

        Ok(())
    } // End of #[cfg(target_os = "linux")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acceleration_display() {
        assert_eq!(Acceleration::Metal.display_name(), "Metal");
        assert_eq!(Acceleration::Cuda.display_name(), "CUDA");
        assert_eq!(Acceleration::Cpu.display_name(), "CPU");
    }

    #[test]
    fn test_acceleration_cmake_flags() {
        assert_eq!(Acceleration::Metal.cmake_flags(), vec!["-DGGML_METAL=ON"]);
        assert_eq!(Acceleration::Cuda.cmake_flags(), vec!["-DGGML_CUDA=ON"]);
        assert!(Acceleration::Cpu.cmake_flags().is_empty());
    }

    #[test]
    fn test_detect_optimal_acceleration() {
        // Just ensure it doesn't panic
        let accel = detect_optimal_acceleration();
        assert!(matches!(
            accel,
            Acceleration::Metal | Acceleration::Cuda | Acceleration::Cpu
        ));
    }

    #[test]
    fn test_get_num_cores() {
        let cores = get_num_cores();
        assert!(cores > 0);
        assert!(cores <= 128); // Reasonable upper bound
    }
}
