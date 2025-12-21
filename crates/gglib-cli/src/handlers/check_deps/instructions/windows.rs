//! Windows installation instructions.

use super::common::{BOLD, RESET, print_command, print_header, print_subsection};
use gglib_core::utils::system::Dependency;

/// Print Windows-specific installation instructions.
pub fn print_instructions(missing: &[&Dependency]) {
    print_header("Windows");

    // Check for winget/choco packages
    let needs_git = missing.iter().any(|d| d.name == "git");
    let needs_cmake = missing.iter().any(|d| d.name == "cmake");
    let needs_python = missing.iter().any(|d| d.name == "python3");
    let needs_curl = missing.iter().any(|d| d.name == "curl");

    if needs_git || needs_cmake || needs_python || needs_curl {
        print_subsection("Install via winget (recommended)");

        if needs_git {
            print_command("winget install Git.Git");
        }
        if needs_cmake {
            print_command("winget install Kitware.CMake");
        }
        if needs_python {
            print_command("winget install Python.Python.3.11");
        }
        if needs_curl {
            println!("  Note: curl is included with Windows 10/11");
        }

        println!();
        print_subsection("Alternative: Install via Chocolatey");

        let mut choco_packages = Vec::new();
        if needs_git {
            choco_packages.push("git");
        }
        if needs_cmake {
            choco_packages.push("cmake");
        }
        if needs_python {
            choco_packages.push("python");
        }

        if !choco_packages.is_empty() {
            print_command(&format!("choco install {}", choco_packages.join(" ")));
        }
    }

    // Rust
    if missing
        .iter()
        .any(|d| d.name == "cargo" || d.name == "rustc")
    {
        print_subsection("Install Rust");
        println!("  Download and run rustup-init.exe from:");
        println!("  https://rustup.rs/");
        println!();
        println!("  Or via winget:");
        print_command("winget install Rustlang.Rustup");
    }

    // Node.js
    if missing.iter().any(|d| d.name == "node" || d.name == "npm") {
        print_subsection("Install Node.js");
        print_command("winget install OpenJS.NodeJS.LTS");
        println!();
        println!("  Or download from: https://nodejs.org/");
    }

    // Visual Studio Build Tools (for make, cc)
    if missing.iter().any(|d| d.name == "make" || d.name == "cc") {
        print_subsection("Install Visual Studio Build Tools");
        println!("  Download Visual Studio Build Tools from:");
        println!("  https://visualstudio.microsoft.com/visual-cpp-build-tools/");
        println!();
        println!("  Select 'Desktop development with C++' workload");
        println!();
        println!("  Alternative: Install MSYS2 for Unix-like tools:");
        print_command("winget install MSYS2.MSYS2");
        println!("  Then in MSYS2 terminal:");
        print_command("pacman -S mingw-w64-x86_64-toolchain make cmake");
    }

    // GPU notes
    println!("\n{}GPU Support:{}", BOLD, RESET);
    println!();
    println!("  {}NVIDIA GPU:{}", BOLD, RESET);
    println!("  Install CUDA Toolkit from:");
    println!("  https://developer.nvidia.com/cuda-downloads");
    println!();
    println!("  {}AMD GPU:{}", BOLD, RESET);
    println!("  Install ROCm for Windows (if supported):");
    println!("  https://rocm.docs.amd.com/");
}
