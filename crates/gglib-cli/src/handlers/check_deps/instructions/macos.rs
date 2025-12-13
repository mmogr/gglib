//! macOS installation instructions.

use super::common::{print_command, print_header, print_subsection};
use gglib_core::utils::system::Dependency;

/// Print macOS-specific installation instructions.
pub fn print_instructions(missing: &[&Dependency]) {
    print_header("macOS");

    // Check if Homebrew is needed
    let needs_brew = missing.iter().any(|d| {
        matches!(
            d.name.as_str(),
            "git" | "cmake" | "python3" | "curl" | "pkg-config"
        )
    });

    if needs_brew {
        print_subsection("Install Homebrew (if not installed)");
        print_command(
            r#"/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)""#,
        );
    }

    // Collect brew packages
    let brew_packages: Vec<&str> = missing
        .iter()
        .filter_map(|d| match d.name.as_str() {
            "git" => Some("git"),
            "cmake" => Some("cmake"),
            "python3" => Some("python3"),
            "curl" => Some("curl"),
            "pkg-config" => Some("pkg-config"),
            _ => None,
        })
        .collect();

    if !brew_packages.is_empty() {
        print_subsection("Install via Homebrew");
        print_command(&format!("brew install {}", brew_packages.join(" ")));
    }

    // Rust
    if missing
        .iter()
        .any(|d| d.name == "cargo" || d.name == "rustc")
    {
        print_subsection("Install Rust");
        print_command("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh");
        println!("  Then restart your terminal or run:");
        print_command("source $HOME/.cargo/env");
    }

    // Node.js
    if missing.iter().any(|d| d.name == "node" || d.name == "npm") {
        print_subsection("Install Node.js");
        println!("  Option 1 - via Homebrew:");
        print_command("brew install node");
        println!();
        println!("  Option 2 - via nvm (recommended for version management):");
        print_command(
            "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash",
        );
        print_command("nvm install --lts");
    }

    // Xcode Command Line Tools (for make, cc)
    if missing.iter().any(|d| d.name == "make" || d.name == "cc") {
        print_subsection("Install Xcode Command Line Tools");
        print_command("xcode-select --install");
    }

    // GPU notes
    println!(
        "\n{}GPU Support:{}",
        super::common::BOLD,
        super::common::RESET
    );
    println!("  Apple Silicon Macs have native Metal GPU support.");
    println!("  No additional drivers needed - llama.cpp will use Metal automatically.");
}
