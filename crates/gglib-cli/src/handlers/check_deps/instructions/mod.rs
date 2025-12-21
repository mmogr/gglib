#![doc = include_str!("README.md")]

//! Installation instructions by platform.
//!
//! This module provides platform-specific installation instructions
//! for missing dependencies.

mod common;
mod linux;
mod macos;
mod windows;

use crate::handlers::check_deps::platform::{Os, detect_linux_distro, detect_os};
use gglib_core::utils::system::Dependency;

/// Print installation instructions for missing dependencies.
pub fn print_installation_instructions(missing: &[&Dependency]) {
    match detect_os() {
        Os::MacOS => macos::print_instructions(missing),
        Os::Windows => windows::print_instructions(missing),
        Os::Linux => {
            let distro = detect_linux_distro();
            linux::print_instructions(missing, distro);
        }
    }
}
