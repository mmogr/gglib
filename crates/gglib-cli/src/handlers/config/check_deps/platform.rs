//! Platform detection utilities.
//!
//! The OS is `gglib_core::host::HostOs`, the value the dependency
//! report itself is made for, so the instructions and the report cannot name
//! different platforms.

/// Detect the Linux distribution family.
///
/// Re-exported from `gglib_runtime` rather than reimplemented, so this check
/// and the llama.cpp build's dependency hint name the same distribution on
/// the same machine.
pub(super) use gglib_runtime::system::detect_linux_distro;

#[cfg(test)]
mod tests {
    use super::*;

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
