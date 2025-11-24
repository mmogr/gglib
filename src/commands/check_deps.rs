//! Check system dependencies command implementation.
//!
//! This module handles checking for required system dependencies
//! and displaying them in a formatted, user-friendly way.

use crate::commands::download::ensure_fast_helper_ready;
use crate::utils::system::{Dependency, DependencyStatus, check_all_dependencies};
use anyhow::{Context, Result};

// ANSI color codes for better UX
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Handles the "check-deps" command to verify system dependencies.
///
/// This function checks for all required and optional dependencies,
/// displays them in a formatted table, and returns an appropriate
/// exit code based on whether all required dependencies are present.
///
/// # Returns
///
/// Returns `Ok(())` if all required dependencies are present.
/// Returns an error if any required dependencies are missing.
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::commands::check_deps::handle_check_deps;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     handle_check_deps().await?;
///     Ok(())
/// }
/// ```
pub async fn handle_check_deps() -> Result<()> {
    println!("{}{}Checking system dependencies...{}\n", BOLD, BLUE, RESET);

    let dependencies = check_all_dependencies();

    println!(
        "{}{:<20} {:<15} {:<50}{}",
        BOLD, "DEPENDENCY", "STATUS", "NOTES", RESET
    );
    println!("{}", "=".repeat(85));

    for dep in &dependencies {
        print_dependency(dep);
    }

    println!();

    let missing_required: Vec<&Dependency> = dependencies
        .iter()
        .filter(|d| d.required && matches!(d.status, DependencyStatus::Missing))
        .collect();

    let present_required = dependencies
        .iter()
        .filter(|d| d.required && matches!(d.status, DependencyStatus::Present { .. }))
        .count();
    let total_required = dependencies.iter().filter(|d| d.required).count();

    println!("{}", "=".repeat(85));
    if missing_required.is_empty() {
        println!(
            "{}✓ All required dependencies are installed!{} ({}/{})",
            GREEN, RESET, present_required, total_required
        );

        println!(
            "{}Ensuring fast download helper is installed...{}",
            BOLD, RESET
        );
        ensure_fast_helper_ready()
            .await
            .context("Failed to set up the Python fast download helper")?;
        println!("{}✓ Fast download helper ready{}", GREEN, RESET);

        print_gpu_status(&dependencies);

        println!("\n{}You can now run: {}make setup{}", BOLD, BLUE, RESET);
        Ok(())
    } else {
        println!(
            "{}✗ {} required dependencies are missing.{} ({}/{})",
            RED,
            missing_required.len(),
            RESET,
            present_required,
            total_required
        );
        println!();
        print_installation_instructions(&missing_required);
        anyhow::bail!("Missing required dependencies")
    }
}

/// Print GPU acceleration status and recommendations
fn print_gpu_status(_dependencies: &[Dependency]) {
    use crate::utils::system::detect_gpu_info;

    println!();

    let gpu_info = detect_gpu_info();

    if gpu_info.has_metal {
        // macOS with Metal
        println!(
            "{}🚀 GPU Acceleration: {}Apple Metal available{}",
            BOLD, GREEN, RESET
        );
        println!(
            "   {}make setup{} will automatically build llama.cpp with Metal support",
            YELLOW, RESET
        );
    } else if let Some(cuda_version) = &gpu_info.cuda_version {
        // CUDA installed and ready
        println!(
            "{}🚀 GPU Acceleration: {}NVIDIA CUDA {} ready!{}",
            BOLD, GREEN, cuda_version, RESET
        );
        println!(
            "   {}make setup{} will automatically build llama.cpp with CUDA support",
            YELLOW, RESET
        );
    } else if gpu_info.has_nvidia_gpu {
        // NVIDIA GPU present but CUDA not installed
        println!(
            "{}⚡ GPU Acceleration: {}NVIDIA GPU detected!{}",
            BOLD, YELLOW, RESET
        );
        println!(
            "   {}To enable GPU acceleration, install CUDA toolkit first:{}",
            BOLD, RESET
        );
        println!(
            "   {}https://developer.nvidia.com/cuda-downloads{}",
            YELLOW, RESET
        );
        println!(
            "   Then run {}make setup{} to build with GPU support",
            YELLOW, RESET
        );
    } else {
        // No GPU detected - CPU only
        println!(
            "{}💻 GPU Acceleration: {}Not detected (CPU-only mode){}",
            BOLD, YELLOW, RESET
        );
        println!(
            "   {}make setup{} will build CPU-only version (works but slower)",
            YELLOW, RESET
        );
        println!("   For faster inference, consider:");
        println!("     • Getting NVIDIA GPU + installing CUDA");
        println!("     • Using cloud GPU instances (AWS, GCP, RunPod, etc.)");
    }
}

/// Print a single dependency row
fn print_dependency(dep: &Dependency) {
    let (status_symbol, status_text, color) = match &dep.status {
        DependencyStatus::Present { version } => ("✓", version.as_str(), GREEN),
        DependencyStatus::Missing => ("✗", "MISSING", RED),
        DependencyStatus::Optional => ("⚠", "optional", YELLOW),
    };

    println!(
        "{:<20} {}{}{:<2} {:<12}{} {:<50}",
        dep.name, color, status_symbol, "", status_text, RESET, dep.description
    );
}

/// Detect the current operating system
fn detect_os() -> &'static str {
    #[cfg(target_os = "macos")]
    return "macos";
    #[cfg(target_os = "windows")]
    return "windows";
    #[cfg(target_os = "linux")]
    return "linux";
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return "unknown";
}

/// Detect Linux distribution (if on Linux)
fn detect_linux_distro() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        // Try to read /etc/os-release
        if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
            if contents.contains("Ubuntu") || contents.contains("Debian") {
                return "debian";
            } else if contents.contains("Fedora") {
                return "fedora";
            } else if contents.contains("Arch") {
                return "arch";
            } else if contents.contains("openSUSE") {
                return "suse";
            }
        }
        "linux-unknown"
    }
    #[cfg(not(target_os = "linux"))]
    return "n/a";
}

/// Print installation instructions for missing dependencies
fn print_installation_instructions(missing: &[&Dependency]) {
    println!("{}{}Installation Instructions:{}", BOLD, BLUE, RESET);
    println!();

    let os = detect_os();
    let distro = if os == "linux" {
        detect_linux_distro()
    } else {
        "n/a"
    };

    // Show platform info
    let platform_name = match (os, distro) {
        ("macos", _) => "macOS",
        ("windows", _) => "Windows",
        ("linux", "debian") => "Ubuntu/Debian",
        ("linux", "fedora") => "Fedora",
        ("linux", "arch") => "Arch Linux",
        ("linux", "suse") => "openSUSE",
        ("linux", _) => "Linux",
        _ => "Unknown",
    };
    println!("{}Platform detected: {}{}", BOLD, platform_name, RESET);
    println!();

    // Group dependencies by type
    let mut rust_deps = Vec::new();
    let mut python_deps = Vec::new();
    let mut node_deps = Vec::new();
    let mut build_deps = Vec::new();
    let mut tauri_deps = Vec::new();

    for dep in missing {
        match dep.name.as_str() {
            "cargo" | "rustc" => rust_deps.push(dep),
            "python3" => python_deps.push(dep),
            "node" | "npm" => node_deps.push(dep),
            "git" | "make" | "gcc" | "g++" | "pkg-config" | "libssl-dev" => build_deps.push(dep),
            "webkit2gtk-4.1" | "librsvg" => tauri_deps.push(dep),
            _ => {}
        }
    }

    let mut step = 1;

    // 1. Rust installation (cross-platform)
    if !rust_deps.is_empty() {
        println!("{}{}. Install Rust toolchain:{}", BOLD, step, RESET);
        match os {
            "windows" => {
                println!("   {}Download and run:{}", YELLOW, RESET);
                println!("   https://win.rustup.rs/x86_64");
            }
            _ => {
                println!(
                    "   {}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh{}",
                    YELLOW, RESET
                );
            }
        }
        println!();
        step += 1;
    }

    // 2. Python installation (cross-platform)
    if !python_deps.is_empty() {
        println!(
            "{}{}. Install Python 3 via Miniconda (recommended for hf_xet helper):{}",
            BOLD, step, RESET
        );
        match (os, distro) {
            ("macos", _) => {
                println!("   {}# Download Miniconda:{}", YELLOW, RESET);
                println!(
                    "   curl -fsS https://repo.anaconda.com/miniconda/Miniconda3-latest-MacOSX-arm64.sh -o miniconda.sh"
                );
                println!(
                    "   bash miniconda.sh -b -p $HOME/miniconda && source $HOME/miniconda/bin/activate"
                );
            }
            ("windows", _) => {
                println!("   {}Download Miniconda (64-bit):{}", YELLOW, RESET);
                println!(
                    "   https://repo.anaconda.com/miniconda/Miniconda3-latest-Windows-x86_64.exe"
                );
                println!(
                    "   {}Enable 'Add Miniconda to PATH' during setup{}",
                    YELLOW, RESET
                );
            }
            ("linux", "debian") => {
                println!(
                    "   {}curl -fsS https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh -o miniconda.sh{}",
                    YELLOW, RESET
                );
                println!(
                    "   bash miniconda.sh -b -p $HOME/miniconda && source $HOME/miniconda/bin/activate"
                );
            }
            ("linux", "fedora") => {
                println!(
                    "   {}curl -fsS https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh -o miniconda.sh{}",
                    YELLOW, RESET
                );
                println!(
                    "   bash miniconda.sh -b -p $HOME/miniconda && source $HOME/miniconda/bin/activate"
                );
            }
            ("linux", "arch") => {
                println!(
                    "   {}curl -fsS https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh -o miniconda.sh{}",
                    YELLOW, RESET
                );
                println!(
                    "   bash miniconda.sh -b -p $HOME/miniconda && source $HOME/miniconda/bin/activate"
                );
            }
            _ => {
                println!(
                    "   {}Install Miniconda from https://docs.conda.io/en/latest/miniconda.html{}",
                    YELLOW, RESET
                );
            }
        }
        println!();
        step += 1;
    }

    // 3. Node.js installation (platform-specific)
    if !node_deps.is_empty() {
        println!("{}{}. Install Node.js:{}", BOLD, step, RESET);
        match (os, distro) {
            ("macos", _) => {
                println!("   {}# Using Homebrew:{}", YELLOW, RESET);
                println!("   brew install node");
            }
            ("windows", _) => {
                println!("   {}Download installer from:{}", YELLOW, RESET);
                println!("   https://nodejs.org");
            }
            ("linux", "debian") => {
                println!("   {}# Ubuntu/Debian:{}", YELLOW, RESET);
                println!("   curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -");
                println!("   sudo apt install -y nodejs");
            }
            ("linux", "fedora") => {
                println!("   {}# Fedora:{}", YELLOW, RESET);
                println!("   sudo dnf install -y nodejs npm");
            }
            ("linux", "arch") => {
                println!("   {}# Arch Linux:{}", YELLOW, RESET);
                println!("   sudo pacman -S nodejs npm");
            }
            _ => {
                println!("   {}Visit: https://nodejs.org{}", YELLOW, RESET);
            }
        }
        println!();
        step += 1;
    }

    // 4. Build tools (platform-specific)
    if !build_deps.is_empty() {
        println!("{}{}. Install build tools:{}", BOLD, step, RESET);
        match (os, distro) {
            ("macos", _) => {
                println!("   {}# Install Xcode Command Line Tools:{}", YELLOW, RESET);
                println!("   xcode-select --install");
                println!();
                println!("   {}# Using Homebrew (if needed):{}", YELLOW, RESET);
                println!("   brew install pkg-config openssl");
            }
            ("windows", _) => {
                println!("   {}# Install Visual Studio Build Tools:{}", YELLOW, RESET);
                println!("   https://visualstudio.microsoft.com/downloads/");
                println!("   (Select 'Desktop development with C++')");
                println!();
                println!("   {}# Install Git:{}", YELLOW, RESET);
                println!("   https://git-scm.com/download/win");
            }
            ("linux", "debian") => {
                println!("   {}# Ubuntu/Debian:{}", YELLOW, RESET);
                println!("   sudo apt update && sudo apt install -y \\");
                println!("     build-essential git pkg-config libssl-dev");
            }
            ("linux", "fedora") => {
                println!("   {}# Fedora:{}", YELLOW, RESET);
                println!("   sudo dnf groupinstall -y 'Development Tools'");
                println!("   sudo dnf install -y git pkg-config openssl-devel");
            }
            ("linux", "arch") => {
                println!("   {}# Arch Linux:{}", YELLOW, RESET);
                println!("   sudo pacman -S base-devel git pkg-config openssl");
            }
            _ => {
                println!(
                    "   {}Install: git, make, gcc, g++, pkg-config, openssl-dev{}",
                    YELLOW, RESET
                );
            }
        }
        println!();
        step += 1;
    }

    // 5. GTK/Tauri dependencies (Linux-specific)
    if !tauri_deps.is_empty() {
        println!("{}{}. Install GTK/Tauri dependencies:{}", BOLD, step, RESET);
        match (os, distro) {
            ("macos", _) => {
                println!(
                    "   {}# macOS: Tauri uses native WebView (no GTK needed){}",
                    GREEN, RESET
                );
            }
            ("windows", _) => {
                println!(
                    "   {}# Windows: Tauri uses WebView2 (included in Windows 11+){}",
                    GREEN, RESET
                );
                println!(
                    "   {}# For Windows 10, download WebView2 Runtime:{}",
                    YELLOW, RESET
                );
                println!("   https://developer.microsoft.com/microsoft-edge/webview2/");
            }
            ("linux", "debian") => {
                println!("   {}# Ubuntu/Debian:{}", YELLOW, RESET);
                println!(
                    "   {}# See: https://v2.tauri.app/start/prerequisites/{}",
                    BLUE, RESET
                );
                println!("   sudo apt update && sudo apt install -y \\");
                println!("     libwebkit2gtk-4.1-dev \\");
                println!("     librsvg2-dev \\");
                println!("     libgtk-3-dev \\");
                println!("     libayatana-appindicator3-dev");
            }
            ("linux", "fedora") => {
                println!("   {}# Fedora:{}", YELLOW, RESET);
                println!("   sudo dnf install -y \\");
                println!("     webkit2gtk4.1-devel \\");
                println!("     librsvg2-devel \\");
                println!("     gtk3-devel \\");
                println!("     libappindicator-gtk3-devel");
            }
            ("linux", "arch") => {
                println!("   {}# Arch Linux:{}", YELLOW, RESET);
                println!("   sudo pacman -S \\");
                println!("     webkit2gtk-4.1 \\");
                println!("     librsvg \\");
                println!("     gtk3 \\");
                println!("     libappindicator-gtk3");
            }
            _ => {
                println!(
                    "   {}Install WebKit2GTK and librsvg development packages{}",
                    YELLOW, RESET
                );
                println!(
                    "   {}See: https://v2.tauri.app/start/prerequisites/{}",
                    BLUE, RESET
                );
            }
        }
        println!();
    }

    // Summary with one-liner install commands
    println!("{}Quick Install (copy-paste):{}", BOLD, RESET);
    match (os, distro) {
        ("macos", _) => {
            if !rust_deps.is_empty() {
                println!(
                    "  {}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh{}",
                    YELLOW, RESET
                );
            }
            if !python_deps.is_empty() {
                println!(
                    "  {}Install Miniconda: https://docs.conda.io/en/latest/miniconda.html{}",
                    YELLOW, RESET
                );
            }
            if !node_deps.is_empty() || !build_deps.is_empty() {
                let mut pkgs = Vec::new();
                if !node_deps.is_empty() {
                    pkgs.push("node");
                }
                if !build_deps.is_empty() {
                    pkgs.extend(&["pkg-config", "openssl"]);
                }
                println!("  {}brew install {}{}", YELLOW, pkgs.join(" "), RESET);
            }
        }
        ("windows", _) => {
            let mut step_idx = 1;
            println!(
                "  {}{}. Install Rust: https://win.rustup.rs/x86_64{}",
                YELLOW, step_idx, RESET
            );
            step_idx += 1;
            if !python_deps.is_empty() {
                println!(
                    "  {}{}. Install Miniconda: https://docs.conda.io/en/latest/miniconda.html{}",
                    YELLOW, step_idx, RESET
                );
                step_idx += 1;
            }
            println!(
                "  {}{}. Install Node.js: https://nodejs.org{}",
                YELLOW, step_idx, RESET
            );
            step_idx += 1;
            println!(
                "  {}{}. Install VS Build Tools: https://visualstudio.microsoft.com/downloads/{}",
                YELLOW, step_idx, RESET
            );
        }
        ("linux", "debian") => {
            let mut cmd = String::from("sudo apt update && sudo apt install -y");
            if !node_deps.is_empty() {
                cmd.push_str(" nodejs npm");
            }
            if !build_deps.is_empty() {
                cmd.push_str(" build-essential git pkg-config libssl-dev");
            }
            if !tauri_deps.is_empty() {
                cmd.push_str(
                    " libwebkit2gtk-4.1-dev librsvg2-dev libgtk-3-dev libayatana-appindicator3-dev",
                );
            }
            if !rust_deps.is_empty() {
                println!(
                    "  {}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh{}",
                    YELLOW, RESET
                );
            }
            if !python_deps.is_empty() {
                println!(
                    "  {}Install Miniconda: https://docs.conda.io/en/latest/miniconda.html{}",
                    YELLOW, RESET
                );
            }
            if !node_deps.is_empty() || !build_deps.is_empty() || !tauri_deps.is_empty() {
                println!("  {}{}{}", YELLOW, cmd, RESET);
            }
        }
        ("linux", "fedora") => {
            if !rust_deps.is_empty() {
                println!(
                    "  {}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh{}",
                    YELLOW, RESET
                );
            }
            if !build_deps.is_empty() {
                println!(
                    "  {}sudo dnf groupinstall -y 'Development Tools' && sudo dnf install -y git pkg-config openssl-devel{}",
                    YELLOW, RESET
                );
            }
            if !node_deps.is_empty() {
                println!("  {}sudo dnf install -y nodejs npm{}", YELLOW, RESET);
            }
            if !tauri_deps.is_empty() {
                println!(
                    "  {}sudo dnf install -y webkit2gtk4.1-devel librsvg2-devel gtk3-devel{}",
                    YELLOW, RESET
                );
            }
            if !python_deps.is_empty() {
                println!(
                    "  {}Install Miniconda: https://docs.conda.io/en/latest/miniconda.html{}",
                    YELLOW, RESET
                );
            }
        }
        ("linux", "arch") => {
            let mut cmd = String::from("sudo pacman -S");
            if !node_deps.is_empty() {
                cmd.push_str(" nodejs npm");
            }
            if !build_deps.is_empty() {
                cmd.push_str(" base-devel git pkg-config openssl");
            }
            if !tauri_deps.is_empty() {
                cmd.push_str(" webkit2gtk-4.1 librsvg gtk3");
            }
            if !rust_deps.is_empty() {
                println!(
                    "  {}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh{}",
                    YELLOW, RESET
                );
            }
            if !python_deps.is_empty() {
                println!(
                    "  {}Install Miniconda: https://docs.conda.io/en/latest/miniconda.html{}",
                    YELLOW, RESET
                );
            }
            if !node_deps.is_empty() || !build_deps.is_empty() || !tauri_deps.is_empty() {
                println!("  {}{}{}", YELLOW, cmd, RESET);
            }
        }
        _ => {
            println!(
                "  {}See platform-specific instructions above{}",
                YELLOW, RESET
            );
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::system::Dependency;

    #[test]
    fn test_print_dependency_present() {
        let dep = Dependency::required("test", "Test dependency").with_status(
            DependencyStatus::Present {
                version: "1.0.0".to_string(),
            },
        );
        // This just ensures the function doesn't panic
        print_dependency(&dep);
    }

    #[test]
    fn test_print_dependency_missing() {
        let dep =
            Dependency::required("test", "Test dependency").with_status(DependencyStatus::Missing);
        print_dependency(&dep);
    }

    #[test]
    fn test_print_dependency_optional() {
        let dep = Dependency::optional("test", "Test dependency");
        print_dependency(&dep);
    }
}
