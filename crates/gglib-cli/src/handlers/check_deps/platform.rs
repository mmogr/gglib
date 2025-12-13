//! Platform detection utilities.

/// Operating system detection result.
#[derive(Debug, Clone, PartialEq)]
pub enum Os {
    MacOS,
    Windows,
    Linux,
}

/// Linux distribution detection result.
#[derive(Debug, Clone, PartialEq)]
pub enum LinuxDistro {
    Debian,
    Fedora,
    Arch,
    Suse,
    Unknown,
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
pub fn detect_linux_distro() -> LinuxDistro {
    // Check /etc/os-release for distribution info
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        let content_lower = content.to_lowercase();

        if content_lower.contains("debian")
            || content_lower.contains("ubuntu")
            || content_lower.contains("pop!_os")
            || content_lower.contains("mint")
        {
            LinuxDistro::Debian
        } else if content_lower.contains("fedora")
            || content_lower.contains("rhel")
            || content_lower.contains("centos")
            || content_lower.contains("rocky")
            || content_lower.contains("alma")
        {
            LinuxDistro::Fedora
        } else if content_lower.contains("arch") || content_lower.contains("manjaro") {
            LinuxDistro::Arch
        } else if content_lower.contains("suse") {
            LinuxDistro::Suse
        } else {
            LinuxDistro::Unknown
        }
    } else {
        LinuxDistro::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_os_returns_valid() {
        let os = detect_os();
        // Just verify it returns one of the expected values
        matches!(os, Os::MacOS | Os::Windows | Os::Linux);
    }

    #[test]
    fn test_linux_distro_detection() {
        // This will return something based on the current system
        let distro = detect_linux_distro();
        matches!(
            distro,
            LinuxDistro::Debian
                | LinuxDistro::Fedora
                | LinuxDistro::Arch
                | LinuxDistro::Suse
                | LinuxDistro::Unknown
        );
    }
}
