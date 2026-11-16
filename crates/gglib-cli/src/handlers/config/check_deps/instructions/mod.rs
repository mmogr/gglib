#![doc = include_str!("README.md")]

mod common;
mod linux;
mod macos;
mod windows;

use crate::handlers::config::check_deps::platform::detect_linux_distro;
use gglib_core::utils::system::{Dependency, HostOs};

/// Print installation instructions for missing dependencies.
pub(super) fn print_installation_instructions(missing: &[&Dependency]) {
    match HostOs::this() {
        HostOs::MacOs => macos::print_instructions(missing),
        HostOs::Windows => windows::print_instructions(missing),
        HostOs::Linux => {
            let distro = detect_linux_distro();
            linux::print_instructions(missing, distro);
        }
    }
}
