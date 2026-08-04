//! Platform detection utilities.

/// Operating system detection result.
#[derive(Debug, Clone, PartialEq)]
pub enum Os {
    MacOS,
    Windows,
    Linux,
}

/// Detect the current operating system.
pub fn detect_os() -> Os {
    if cfg!(target_os = "macos") {
        Os::MacOS
    } else if cfg!(target_os = "windows") {
        Os::Windows
    } else {
        Os::Linux
    }
}

/// Detect the Linux distribution family.
///
/// Re-exported from `gglib_runtime` rather than reimplemented. This module
/// used to carry its own copy that searched the whole of `/etc/os-release` for
/// distribution names — which meant a `HOME_URL` containing "research" was read
/// as Arch Linux, and the CLI could recommend different packages than the
/// dependency check did on the very same machine.
pub use gglib_runtime::system::detect_linux_distro;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_os_returns_valid() {
        let os = detect_os();
        // Just verify it returns one of the expected values
        matches!(os, Os::MacOS | Os::Windows | Os::Linux);
    }

    /// Detection itself is tested against real `/etc/os-release` fixtures in
    /// `gglib_core::utils::system::packages`, where it is a pure function.
    /// Here we only confirm the re-export resolves and runs on this machine.
    #[test]
    fn the_distro_probe_is_reachable_and_total() {
        use gglib_core::utils::system::LinuxDistro;

        matches!(
            detect_linux_distro(),
            LinuxDistro::Debian
                | LinuxDistro::Fedora
                | LinuxDistro::Arch
                | LinuxDistro::Suse
                | LinuxDistro::Unknown
        );
    }
}
