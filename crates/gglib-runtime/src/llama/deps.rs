//! Dependency checking and validation for building llama.cpp.

#[cfg(feature = "cli")]
use super::detect::{has_cmake, has_cpp_compiler, has_git};
#[cfg(feature = "cli")]
use anyhow::{Result, bail};

/// Check if all required build dependencies are installed
#[cfg(feature = "cli")]
pub fn check_dependencies() -> Result<DependencyStatus> {
    println!("Checking build dependencies...");

    let git = has_git()?;
    let cmake = has_cmake()?;
    let compiler = has_cpp_compiler()?;

    let git_ok = git.is_some();
    let cmake_ok = cmake.is_some();
    let compiler_ok = compiler.is_some();

    // Print status
    if git_ok {
        println!("✓ git (version {})", git.as_ref().unwrap());
    } else {
        println!("✗ git not found");
    }

    if cmake_ok {
        println!("✓ cmake (version {})", cmake.as_ref().unwrap());
    } else {
        println!("✗ cmake not found");
    }

    if compiler_ok {
        println!("✓ C++ compiler {}", compiler.as_ref().unwrap());
    } else {
        println!("✗ C++ compiler not found");
    }

    let all_ok = git_ok && cmake_ok && compiler_ok;

    if !all_ok {
        println!();
        print_installation_instructions();
        bail!("Missing required build dependencies");
    }

    Ok(DependencyStatus {
        _git: git.unwrap(),
        _cmake: cmake.unwrap(),
        _compiler: compiler.unwrap(),
    })
}

/// Information about installed dependencies.
/// Fields prefixed with _ as struct is returned but fields not currently read.
#[cfg(feature = "cli")]
#[derive(Debug)]
pub struct DependencyStatus {
    pub _git: String,
    pub _cmake: String,
    pub _compiler: String,
}

/// Print platform-specific installation instructions for missing dependencies
#[cfg(feature = "cli")]
fn print_installation_instructions() {
    println!("Missing dependencies detected. Please install:");
    println!();

    #[cfg(target_os = "macos")]
    {
        println!("macOS:");
        println!("  xcode-select --install");
        println!("  brew install cmake git");
    }

    #[cfg(target_os = "linux")]
    {
        // The build needs a compiler, cmake and git; the shared package table
        // knows what each is called here. Naming them through it rather than
        // inline is what keeps this in step with `gglib config check-deps`,
        // which used to print different package names for the same thing.
        let distro = crate::system::detect_linux_distro();
        println!("{}:", distro.label());

        let packages: Vec<&str> = ["gcc", "cmake", "git"]
            .iter()
            .filter_map(|dependency| {
                gglib_core::utils::system::packages_for(dependency)
                    .and_then(|names| names.for_distro(distro))
            })
            .collect();

        match (distro.installer(), packages.is_empty()) {
            (Some(installer), false) => println!("  sudo {installer} {}", packages.join(" ")),
            _ => println!("  Install: a C/C++ toolchain, cmake, git"),
        }
    }

    #[cfg(target_os = "windows")]
    {
        println!("Windows:");
        println!("  1. Install Visual Studio 2022 with C++ tools");
        println!("     https://visualstudio.microsoft.com/downloads/");
        println!("  2. Install CMake from: https://cmake.org/download/");
        println!("  3. Install Git from: https://git-scm.com/download/win");
    }

    println!();
    println!("After installing, run 'gglib config llama install' again.");
}

/// Check available disk space
#[cfg(feature = "cli")]
pub fn check_disk_space(_required_mb: u64) -> Result<bool> {
    use gglib_core::paths::data_root;
    use std::fs;

    let gglib_dir = data_root().map_err(|e| anyhow::anyhow!("{}", e))?;

    // Create directory if it doesn't exist
    if !gglib_dir.exists() {
        fs::create_dir_all(&gglib_dir)?;
    }

    // Try to get available space (platform-specific)
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(metadata) = fs::metadata(&gglib_dir) {
            // This is a simplified check - doesn't actually get free space
            // In a real implementation, you'd use statvfs or similar
            let _ = metadata.blocks(); // Placeholder
        }
        Ok(true) // Assume enough space for now
    }

    #[cfg(windows)]
    {
        // On Windows, you'd use GetDiskFreeSpaceEx
        // For now, assume enough space
        return Ok(true);
    }

    #[cfg(not(any(unix, windows)))]
    {
        // For other platforms, assume enough space
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "cli")]
    use super::*;

    #[test]
    #[cfg(feature = "cli")]
    fn test_check_disk_space() {
        // Should not panic
        let result = check_disk_space(800);
        assert!(result.is_ok());
    }
}
