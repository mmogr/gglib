//! Linux installation instructions.

use super::common::{BOLD, RESET, print_command, print_header, print_subsection};
use crate::handlers::check_deps::platform::LinuxDistro;
use gglib_core::utils::system::Dependency;

/// Print Linux-specific installation instructions.
pub fn print_instructions(missing: &[&Dependency], distro: LinuxDistro) {
    let distro_name = match distro {
        LinuxDistro::Debian => "Debian/Ubuntu",
        LinuxDistro::Fedora => "Fedora/RHEL",
        LinuxDistro::Arch => "Arch Linux",
        LinuxDistro::Suse => "openSUSE",
        LinuxDistro::Unknown => "Linux",
    };

    print_header(distro_name);

    match distro {
        LinuxDistro::Debian => print_debian_instructions(missing),
        LinuxDistro::Fedora => print_fedora_instructions(missing),
        LinuxDistro::Arch => print_arch_instructions(missing),
        LinuxDistro::Suse => print_suse_instructions(missing),
        LinuxDistro::Unknown => print_generic_instructions(missing),
    }

    // Common instructions for Rust and Node.js
    print_common_linux_instructions(missing);

    // GPU notes
    print_gpu_notes(&distro);
}

fn print_debian_instructions(missing: &[&Dependency]) {
    let apt_packages: Vec<&str> = missing
        .iter()
        .filter_map(|d| match d.name.as_str() {
            "git" => Some("git"),
            "cmake" => Some("cmake"),
            "python3" => Some("python3"),
            "curl" => Some("curl"),
            "make" => Some("build-essential"),
            "cc" => Some("build-essential"),
            "pkg-config" => Some("pkg-config"),
            _ => None,
        })
        .collect();

    if !apt_packages.is_empty() {
        // Deduplicate (build-essential might appear twice)
        let mut unique_packages: Vec<&str> = apt_packages.clone();
        unique_packages.sort();
        unique_packages.dedup();

        print_subsection("Install via apt");
        print_command("sudo apt update");
        print_command(&format!(
            "sudo apt install -y {}",
            unique_packages.join(" ")
        ));
    }
}

fn print_fedora_instructions(missing: &[&Dependency]) {
    let dnf_packages: Vec<&str> = missing
        .iter()
        .filter_map(|d| match d.name.as_str() {
            "git" => Some("git"),
            "cmake" => Some("cmake"),
            "python3" => Some("python3"),
            "curl" => Some("curl"),
            "make" => Some("make"),
            "cc" => Some("gcc gcc-c++"),
            "pkg-config" => Some("pkgconfig"),
            _ => None,
        })
        .collect();

    if !dnf_packages.is_empty() {
        print_subsection("Install via dnf");
        print_command(&format!("sudo dnf install -y {}", dnf_packages.join(" ")));
    }

    // Development tools group
    if missing.iter().any(|d| d.name == "make" || d.name == "cc") {
        println!();
        println!("  Or install the development tools group:");
        print_command(r#"sudo dnf groupinstall -y "Development Tools""#);
    }
}

fn print_arch_instructions(missing: &[&Dependency]) {
    let pacman_packages: Vec<&str> = missing
        .iter()
        .filter_map(|d| match d.name.as_str() {
            "git" => Some("git"),
            "cmake" => Some("cmake"),
            "python3" => Some("python"),
            "curl" => Some("curl"),
            "make" => Some("base-devel"),
            "cc" => Some("base-devel"),
            "pkg-config" => Some("pkgconf"),
            _ => None,
        })
        .collect();

    if !pacman_packages.is_empty() {
        let mut unique_packages: Vec<&str> = pacman_packages.clone();
        unique_packages.sort();
        unique_packages.dedup();

        print_subsection("Install via pacman");
        print_command(&format!(
            "sudo pacman -S --needed {}",
            unique_packages.join(" ")
        ));
    }
}

fn print_suse_instructions(missing: &[&Dependency]) {
    let zypper_packages: Vec<&str> = missing
        .iter()
        .filter_map(|d| match d.name.as_str() {
            "git" => Some("git"),
            "cmake" => Some("cmake"),
            "python3" => Some("python3"),
            "curl" => Some("curl"),
            "make" => Some("make"),
            "cc" => Some("gcc gcc-c++"),
            "pkg-config" => Some("pkg-config"),
            _ => None,
        })
        .collect();

    if !zypper_packages.is_empty() {
        print_subsection("Install via zypper");
        print_command(&format!(
            "sudo zypper install -y {}",
            zypper_packages.join(" ")
        ));
    }

    // Pattern for development
    if missing.iter().any(|d| d.name == "make" || d.name == "cc") {
        println!();
        println!("  Or install the development pattern:");
        print_command("sudo zypper install -t pattern devel_basis");
    }
}

fn print_generic_instructions(missing: &[&Dependency]) {
    print_subsection("Package Installation");
    println!("  Your distribution was not auto-detected.");
    println!("  Please install the following packages using your package manager:");
    println!();

    for dep in missing {
        match dep.name.as_str() {
            "git" | "cmake" | "python3" | "curl" | "make" | "pkg-config" => {
                println!("  - {}", dep.name);
            }
            "cc" => {
                println!("  - gcc or clang (C compiler)");
            }
            _ => {}
        }
    }

    println!();
    println!("  Common package manager commands:");
    println!("  - Debian/Ubuntu: sudo apt install <package>");
    println!("  - Fedora/RHEL:   sudo dnf install <package>");
    println!("  - Arch Linux:    sudo pacman -S <package>");
    println!("  - openSUSE:      sudo zypper install <package>");
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

fn print_gpu_notes(distro: &LinuxDistro) {
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
    println!("  Install ROCm:");
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
