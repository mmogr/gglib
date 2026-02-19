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
            "make" | "gcc" | "g++" | "cc" => Some("build-essential"),
            "pkg-config" => Some("pkg-config"),
            "libssl-dev" => Some("libssl-dev"),
            "libclang-dev" => Some("libclang-dev"),
            "libsqlite3-dev" => Some("libsqlite3-dev"),
            "libasound2-dev" => Some("libasound2-dev"),
            "libcurl-dev" => Some("libcurl4-openssl-dev"),
            "webkit2gtk-4.1" => Some("libwebkit2gtk-4.1-dev"),
            "librsvg" => Some("librsvg2-dev"),
            "libappindicator-gtk3" => Some("libayatana-appindicator3-dev"),
            "patchelf" => Some("patchelf"),
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
            "make" | "gcc" | "g++" | "cc" => Some("gcc gcc-c++ make"),
            "pkg-config" => Some("pkgconfig"),
            "libssl-dev" => Some("openssl-devel"),
            "libclang-dev" => Some("clang-devel"),
            "libsqlite3-dev" => Some("sqlite-devel"),
            "libasound2-dev" => Some("alsa-lib-devel"),
            "libcurl-dev" => Some("libcurl-devel"),
            "webkit2gtk-4.1" => Some("webkit2gtk4.1-devel"),
            "librsvg" => Some("librsvg2-devel"),
            "libappindicator-gtk3" => Some("libappindicator-gtk3-devel"),
            "patchelf" => Some("patchelf"),
            _ => None,
        })
        .collect();

    if !dnf_packages.is_empty() {
        print_subsection("Install via dnf");
        print_command(&format!("sudo dnf install -y {}", dnf_packages.join(" ")));
    }

    // Development tools group
    if missing
        .iter()
        .any(|d| matches!(d.name.as_str(), "make" | "gcc" | "g++" | "cc"))
    {
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
            "make" | "gcc" | "g++" | "cc" => Some("base-devel"),
            "pkg-config" => Some("pkgconf"),
            "libssl-dev" => Some("openssl"),
            "libclang-dev" => Some("clang"),
            "libsqlite3-dev" => Some("sqlite"),
            "libasound2-dev" => Some("alsa-lib"),
            "libcurl-dev" => Some("curl"),
            "webkit2gtk-4.1" => Some("webkit2gtk-4.1"),
            "librsvg" => Some("librsvg"),
            "libappindicator-gtk3" => Some("libappindicator-gtk3"),
            "patchelf" => Some("patchelf"),
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
            "make" | "gcc" | "g++" | "cc" => Some("gcc gcc-c++ make"),
            "pkg-config" => Some("pkg-config"),
            "libssl-dev" => Some("libopenssl-devel"),
            "libclang-dev" => Some("clang-devel"),
            "libsqlite3-dev" => Some("sqlite3-devel"),
            "libasound2-dev" => Some("alsa-devel"),
            "libcurl-dev" => Some("libcurl-devel"),
            "webkit2gtk-4.1" => Some("webkit2gtk3-devel"),
            "librsvg" => Some("librsvg-devel"),
            "libappindicator-gtk3" => Some("libappindicator3-devel"),
            "patchelf" => Some("patchelf"),
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
    if missing
        .iter()
        .any(|d| matches!(d.name.as_str(), "make" | "gcc" | "g++" | "cc"))
    {
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
            "gcc" | "g++" | "cc" => {
                println!("  - gcc/g++ (C/C++ compiler)");
            }
            "libssl-dev" => println!("  - OpenSSL development headers"),
            "libclang-dev" => println!("  - libclang development headers"),
            "libsqlite3-dev" => println!("  - SQLite3 development headers"),
            "libasound2-dev" => println!("  - ALSA development headers"),
            "libcurl-dev" => println!("  - libcurl development headers"),
            "webkit2gtk-4.1" => println!("  - WebKit2GTK 4.1 development headers"),
            "librsvg" => println!("  - librsvg development headers"),
            "libappindicator-gtk3" => println!("  - libappindicator-gtk3 development headers"),
            "patchelf" => println!("  - patchelf"),
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
