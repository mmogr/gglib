//! Linux installation instructions.

use super::common::{BOLD, RESET, print_command, print_header, print_subsection};
use gglib_core::utils::system::{Dependency, LinuxDistro, packages_for};

/// Print Linux-specific installation instructions.
pub(super) fn print_instructions(missing: &[&Dependency], distro: LinuxDistro) {
    print_header(distro.label());

    print_package_instructions(missing, distro);

    // Common instructions for Rust and Node.js
    print_common_linux_instructions(missing);

    // GPU notes
    print_gpu_notes(distro);
}

/// Print the one command that installs every missing system package.
///
/// This used to be four near-identical functions — one per distribution —
/// each mapping the same dependency names to their own package names. Keeping
/// them in step was manual, and they had already drifted: three of them still
/// named the pre-Ayatana `libappindicator-gtk3`, which on Arch cannot be
/// installed at all. One table now answers for every distribution.
fn print_package_instructions(missing: &[&Dependency], distro: LinuxDistro) {
    let Some(installer) = distro.installer() else {
        print_unidentified_distro_instructions(missing);
        return;
    };

    let mut packages: Vec<&str> = missing
        .iter()
        .filter_map(|dep| packages_for(&dep.name).and_then(|names| names.for_distro(distro)))
        .collect();

    if packages.is_empty() {
        return;
    }

    // `make`, `gcc` and `g++` all arrive in one toolchain package, so without
    // this the command would name it up to three times.
    packages.sort_unstable();
    packages.dedup();

    let manager = installer.split_whitespace().next().unwrap_or(installer);
    print_subsection(&format!("Install via {manager}"));

    // apt is the one that needs its index refreshed first.
    if distro == LinuxDistro::Debian {
        print_command("sudo apt update");
    }

    print_command(&format!("sudo {installer} {}", packages.join(" ")));
}

/// Name what is missing when we cannot name the packages.
///
/// Reached when `/etc/os-release` identified nothing recognised. Describing
/// the dependency ("OpenSSL development headers") beats guessing a package
/// name: a wrong name is typed in and fails, a description is searched for.
fn print_unidentified_distro_instructions(missing: &[&Dependency]) {
    print_subsection("Package Installation");
    println!("  Your distribution was not recognised.");
    println!("  Please install the following using your package manager:");
    println!();

    let mut described: Vec<&str> = missing
        .iter()
        .filter_map(|dep| packages_for(&dep.name).map(|names| names.generic))
        .collect();
    described.sort_unstable();
    described.dedup();

    for description in described {
        println!("  - {description}");
    }

    println!();
    println!("  Common package manager commands:");
    for distro in [
        LinuxDistro::Debian,
        LinuxDistro::Fedora,
        LinuxDistro::Arch,
        LinuxDistro::Suse,
    ] {
        if let Some(installer) = distro.installer() {
            println!("  - {:<14} sudo {installer} <package>", distro.label());
        }
    }
}

fn print_common_linux_instructions(missing: &[&Dependency]) {
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
        println!("  Option 1 - via nvm (recommended):");
        print_command(
            "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash",
        );
        print_command("nvm install --lts");
        println!();
        println!("  Option 2 - via NodeSource (Debian/Ubuntu):");
        print_command("curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -");
        print_command("sudo apt install -y nodejs");
    }
}

/// GPU driver notes.
///
/// These stay hand-written rather than moving into the package table: they are
/// vendor driver stacks with their own repositories and setup steps, not
/// single packages a dependency name maps onto.
fn print_gpu_notes(distro: LinuxDistro) {
    println!("\n{}GPU Support:{}", BOLD, RESET);

    println!();
    println!("  {}NVIDIA GPU:{}", BOLD, RESET);

    match distro {
        LinuxDistro::Debian => {
            println!("  Install NVIDIA drivers and CUDA:");
            print_command("sudo apt install nvidia-driver-535 nvidia-cuda-toolkit");
        }
        LinuxDistro::Fedora => {
            println!("  Enable RPM Fusion and install:");
            print_command("sudo dnf install akmod-nvidia xorg-x11-drv-nvidia-cuda");
        }
        LinuxDistro::Arch => {
            println!("  Install NVIDIA drivers:");
            print_command("sudo pacman -S nvidia nvidia-utils cuda");
        }
        LinuxDistro::Suse => {
            println!("  Install from NVIDIA repository:");
            println!("  https://en.opensuse.org/SDB:NVIDIA_drivers");
        }
        LinuxDistro::Unknown => {
            println!("  Install CUDA Toolkit from:");
            println!("  https://developer.nvidia.com/cuda-downloads");
        }
    }

    println!();
    println!("  {}AMD GPU:{}", BOLD, RESET);
    println!("  Install Vulkan drivers for GPU acceleration:");

    // The same Vulkan row the dependency check hints with, so the two cannot
    // recommend different packages for the same thing.
    match (
        distro.installer(),
        packages_for("Vulkan").and_then(|names| names.for_distro(distro)),
    ) {
        (Some(installer), Some(packages)) => print_command(&format!("sudo {installer} {packages}")),
        _ => println!("  Install your GPU vendor's Vulkan driver, plus vulkan-tools"),
    }

    println!();
    println!("  Alternatively, install ROCm:");
    println!("  https://rocm.docs.amd.com/projects/install-on-linux/en/latest/");

    match distro {
        LinuxDistro::Debian => {
            print_command("sudo apt install rocm-dev rocm-libs");
        }
        LinuxDistro::Fedora => {
            println!("  Follow AMD's official ROCm installation guide for Fedora");
        }
        LinuxDistro::Arch => {
            print_command("sudo pacman -S rocm-hip-sdk");
        }
        _ => {}
    }
}
