//! Dependency checking and validation for building llama.cpp.

use super::detect::{has_cmake, has_cpp_compiler, has_git};
use anyhow::{Result, bail};

/// Check that the build dependencies are installed, printing what was found.
///
/// Fails, after printing how to install what is missing, unless git, cmake and
/// a C++ compiler are all present.
pub fn check_dependencies() -> Result<()> {
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

    Ok(())
}

/// Print platform-specific installation instructions for missing dependencies
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
        // inline keeps this in step with `gglib config check-deps`, which
        // reads the same table.
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
