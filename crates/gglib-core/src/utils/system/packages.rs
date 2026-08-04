//! Linux distribution identity and the package names that follow from it.
//!
//! The same knowledge — "what is this distro, and what is this dependency
//! called on it" — was previously spelled out in five places: an `/etc/os-release`
//! substring match in the CLI, another in the llama.cpp build checker, four
//! near-identical `match` blocks turning dependency names into apt/dnf/pacman/
//! zypper packages, and twenty hardcoded `apt install` install hints that were
//! simply wrong anywhere else. Keeping five copies in step by hand is what let
//! them drift.
//!
//! Everything here is pure. [`parse_os_release`] takes the file's *contents*
//! rather than reading them, so this stays in the domain layer with no I/O, and
//! the parser can be tested against real files from distributions no CI machine
//! is running.

/// Distribution family, which is what package names actually key off.
///
/// Families rather than distributions: Mint installs like Debian and `CachyOS`
/// installs like Arch, and enumerating every derivative would be a losing race.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxDistro {
    Debian,
    Fedora,
    Arch,
    Suse,
    /// Identified as none of the above — including when `/etc/os-release` was
    /// missing or unreadable, which callers report the same way.
    Unknown,
}

impl LinuxDistro {
    /// Human-readable family name, for headings and instructions.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Debian => "Debian/Ubuntu",
            Self::Fedora => "Fedora/RHEL",
            Self::Arch => "Arch Linux",
            Self::Suse => "openSUSE",
            Self::Unknown => "Linux",
        }
    }

    /// The install command this family uses, without `sudo`.
    ///
    /// `None` for [`Self::Unknown`], where guessing would be worse than saying
    /// so: a wrong command is followed and fails, a missing one is looked up.
    #[must_use]
    pub const fn installer(self) -> Option<&'static str> {
        match self {
            Self::Debian => Some("apt install"),
            Self::Fedora => Some("dnf install"),
            Self::Arch => Some("pacman -S"),
            Self::Suse => Some("zypper install"),
            Self::Unknown => None,
        }
    }
}

/// What one dependency is called on each family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageNames {
    pub debian: &'static str,
    pub fedora: &'static str,
    pub arch: &'static str,
    pub suse: &'static str,
    /// Prose for an unidentified distribution, where a package name would be a
    /// guess but "OpenSSL development headers" is still actionable.
    pub generic: &'static str,
}

impl PackageNames {
    /// The name for `distro`, or `None` when there is no name to give.
    #[must_use]
    pub const fn for_distro(&self, distro: LinuxDistro) -> Option<&'static str> {
        match distro {
            LinuxDistro::Debian => Some(self.debian),
            LinuxDistro::Fedora => Some(self.fedora),
            LinuxDistro::Arch => Some(self.arch),
            LinuxDistro::Suse => Some(self.suse),
            LinuxDistro::Unknown => None,
        }
    }
}

/// Dependency name → package names, keyed by the names used in
/// `SystemProbePort::check_all_dependencies`.
///
/// Several toolchain entries deliberately share a row: `make`, `gcc` and `g++`
/// all arrive with `build-essential` on Debian and `base-devel` on Arch, so
/// callers listing several missing dependencies must de-duplicate the result.
const PACKAGES: &[(&str, PackageNames)] = &[
    (
        "git",
        PackageNames {
            debian: "git",
            fedora: "git",
            arch: "git",
            suse: "git",
            generic: "git",
        },
    ),
    (
        "make",
        PackageNames {
            debian: "build-essential",
            fedora: "gcc gcc-c++ make",
            arch: "base-devel",
            suse: "gcc gcc-c++ make",
            generic: "a C/C++ toolchain (make, gcc, g++)",
        },
    ),
    (
        "gcc",
        PackageNames {
            debian: "build-essential",
            fedora: "gcc gcc-c++ make",
            arch: "base-devel",
            suse: "gcc gcc-c++ make",
            generic: "a C/C++ toolchain (make, gcc, g++)",
        },
    ),
    (
        "g++",
        PackageNames {
            debian: "build-essential",
            fedora: "gcc gcc-c++ make",
            arch: "base-devel",
            suse: "gcc gcc-c++ make",
            generic: "a C/C++ toolchain (make, gcc, g++)",
        },
    ),
    (
        "pkg-config",
        PackageNames {
            debian: "pkg-config",
            fedora: "pkgconfig",
            arch: "pkgconf",
            suse: "pkg-config",
            generic: "pkg-config",
        },
    ),
    (
        "cmake",
        PackageNames {
            debian: "cmake",
            fedora: "cmake",
            arch: "cmake",
            suse: "cmake",
            generic: "cmake",
        },
    ),
    (
        "python3",
        PackageNames {
            debian: "python3",
            fedora: "python3",
            arch: "python",
            suse: "python3",
            generic: "python3",
        },
    ),
    (
        "libssl-dev",
        PackageNames {
            debian: "libssl-dev",
            fedora: "openssl-devel",
            arch: "openssl",
            suse: "libopenssl-devel",
            generic: "OpenSSL development headers",
        },
    ),
    (
        "patchelf",
        PackageNames {
            debian: "patchelf",
            fedora: "patchelf",
            arch: "patchelf",
            suse: "patchelf",
            generic: "patchelf",
        },
    ),
    (
        "webkit2gtk-4.1",
        PackageNames {
            debian: "libwebkit2gtk-4.1-dev",
            fedora: "webkit2gtk4.1-devel",
            arch: "webkit2gtk-4.1",
            suse: "webkit2gtk3-devel",
            generic: "WebKit2GTK 4.1 development headers",
        },
    ),
    (
        "librsvg",
        PackageNames {
            debian: "librsvg2-dev",
            fedora: "librsvg2-devel",
            arch: "librsvg",
            suse: "librsvg-devel",
            generic: "librsvg development headers",
        },
    ),
    (
        // Ayatana rather than the older libappindicator on every family:
        // `check_libappindicator` probes `ayatana-appindicator3-0.1` first, and
        // on Arch the non-Ayatana package left the repositories, so the name
        // this table used to give could not be installed at all.
        "libappindicator-gtk3",
        PackageNames {
            debian: "libayatana-appindicator3-dev",
            fedora: "libayatana-appindicator-gtk3-devel",
            arch: "libayatana-appindicator",
            suse: "libayatana-appindicator3-devel",
            generic: "libayatana-appindicator3 development headers",
        },
    ),
    (
        "libasound2-dev",
        PackageNames {
            debian: "libasound2-dev",
            fedora: "alsa-lib-devel",
            arch: "alsa-lib",
            suse: "alsa-devel",
            generic: "ALSA development headers",
        },
    ),
    (
        "libcurl-dev",
        PackageNames {
            debian: "libcurl4-openssl-dev",
            fedora: "libcurl-devel",
            arch: "curl",
            suse: "libcurl-devel",
            generic: "libcurl development headers",
        },
    ),
    (
        "libsqlite3-dev",
        PackageNames {
            debian: "libsqlite3-dev",
            fedora: "sqlite-devel",
            arch: "sqlite",
            suse: "sqlite3-devel",
            generic: "SQLite3 development headers",
        },
    ),
    (
        "libclang-dev",
        PackageNames {
            debian: "libclang-dev",
            fedora: "clang-devel",
            arch: "clang",
            suse: "clang-devel",
            generic: "libclang development headers",
        },
    ),
    (
        "Vulkan headers",
        PackageNames {
            debian: "libvulkan-dev",
            fedora: "vulkan-loader-devel",
            arch: "vulkan-headers",
            suse: "vulkan-devel",
            generic: "Vulkan development headers",
        },
    ),
    (
        "glslc",
        PackageNames {
            debian: "glslc",
            fedora: "glslc",
            arch: "shaderc",
            suse: "shaderc",
            generic: "glslc (the shaderc SPIR-V compiler)",
        },
    ),
    (
        "SPIR-V headers",
        PackageNames {
            debian: "spirv-headers",
            fedora: "spirv-headers-devel",
            arch: "spirv-headers",
            suse: "spirv-headers",
            generic: "SPIR-V headers",
        },
    ),
    (
        "Vulkan",
        PackageNames {
            debian: "mesa-vulkan-drivers vulkan-tools",
            fedora: "mesa-vulkan-drivers vulkan-tools",
            arch: "vulkan-radeon vulkan-tools",
            suse: "libvulkan_radeon vulkan-tools",
            generic: "your GPU vendor's Vulkan driver, plus vulkan-tools",
        },
    ),
];

/// Package names for a dependency.
///
/// `None` when the dependency does not come from the system package manager at
/// all: `cargo` and `node` have their own installers, and pointing someone at a
/// distribution package for those would be actively unhelpful.
#[must_use]
pub fn packages_for(dependency: &str) -> Option<PackageNames> {
    PACKAGES
        .iter()
        .find(|(name, _)| *name == dependency)
        .map(|(_, packages)| *packages)
}

/// A ready-to-run install command for one dependency, e.g.
/// `pacman -S libayatana-appindicator`.
///
/// `None` when the distribution is unidentified or the dependency is not a
/// system package; callers fall back to [`PackageNames::generic`] prose.
#[must_use]
pub fn install_hint(dependency: &str, distro: LinuxDistro) -> Option<String> {
    let names = packages_for(dependency)?;
    let installer = distro.installer()?;
    let package = names.for_distro(distro)?;

    Some(format!("{installer} {package}"))
}

/// Identify the distribution family from the contents of `/etc/os-release`.
///
/// Reads the `ID` and `ID_LIKE` fields defined by the os-release specification
/// rather than searching the file for distribution names. That distinction is
/// the whole point: `HOME_URL="https://example.org/research/"` contains "arch",
/// and a substring search would call that machine Arch Linux and hand it
/// `pacman` commands.
///
/// `ID` wins over `ID_LIKE`, so a derivative that names itself is taken at its
/// word before its declared kinship is consulted.
#[must_use]
pub fn parse_os_release(contents: &str) -> LinuxDistro {
    let mut id = None;
    let mut id_like = None;

    for line in contents.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("ID=") {
            id = Some(unquote(value));
        } else if let Some(value) = line.strip_prefix("ID_LIKE=") {
            id_like = Some(unquote(value));
        }
    }

    if let Some(family) = id.and_then(family_of) {
        return family;
    }

    // ID_LIKE is a space-separated list, most closely related first, so the
    // first one recognised is the closest match rather than merely any match.
    id_like
        .and_then(|likes| likes.split_whitespace().find_map(family_of))
        .unwrap_or(LinuxDistro::Unknown)
}

/// Strip the optional quoting os-release allows around values.
fn unquote(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value)
}

/// Map a single os-release identifier to its family.
///
/// Derivatives are listed where they are common enough to be worth naming, but
/// the list is not the safety net — `ID_LIKE` is, which is what makes an
/// unlisted derivative work anyway.
fn family_of(id: &str) -> Option<LinuxDistro> {
    match id.to_ascii_lowercase().as_str() {
        "debian" | "ubuntu" | "linuxmint" | "pop" | "raspbian" => Some(LinuxDistro::Debian),
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" => Some(LinuxDistro::Fedora),
        "arch" | "archarm" | "cachyos" | "manjaro" | "endeavouros" => Some(LinuxDistro::Arch),
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "sles" | "suse" => {
            Some(LinuxDistro::Suse)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CachyOS` names itself, so `ID` alone identifies it.
    #[test]
    fn cachyos_is_arch() {
        let os_release = "\
NAME=\"CachyOS Linux\"
PRETTY_NAME=\"CachyOS\"
ID=cachyos
ID_LIKE=arch
ANSI_COLOR=\"38;2;23;147;209\"
HOME_URL=\"https://cachyos.org/\"
";

        assert_eq!(parse_os_release(os_release), LinuxDistro::Arch);
    }

    /// The safety net for derivatives nobody has enumerated: an unrecognised
    /// `ID` still resolves through `ID_LIKE`.
    #[test]
    fn an_unknown_derivative_falls_back_to_its_family() {
        let os_release = "ID=somenewarchspin\nID_LIKE=arch\n";

        assert_eq!(parse_os_release(os_release), LinuxDistro::Arch);
    }

    /// `ID_LIKE` lists the closest relation first, and that is the one to use.
    #[test]
    fn the_closest_relation_in_id_like_wins() {
        let os_release = "ID=ultramarine\nID_LIKE=\"fedora rhel centos\"\n";

        assert_eq!(parse_os_release(os_release), LinuxDistro::Fedora);
    }

    /// The bug that made this a parser rather than a substring search:
    /// "research" contains "arch". A machine like this used to be handed
    /// `pacman` commands.
    #[test]
    fn a_url_containing_arch_does_not_make_it_arch() {
        let os_release = "\
NAME=\"Example Linux\"
ID=example
HOME_URL=\"https://example.org/research/\"
SUPPORT_URL=\"https://example.org/research/support\"
";

        assert_eq!(parse_os_release(os_release), LinuxDistro::Unknown);
    }

    /// Same trap through the other field: `ID_LIKE` is matched token by token,
    /// so a value merely containing "arch" is not a match.
    #[test]
    fn id_like_is_matched_whole_not_by_substring() {
        let os_release = "ID=example\nID_LIKE=monarch\n";

        assert_eq!(parse_os_release(os_release), LinuxDistro::Unknown);
    }

    /// A file with no `ID` at all is unknown rather than a guess.
    #[test]
    fn a_file_without_an_id_is_unknown() {
        let os_release = "NAME=\"Some Linux\"\nVERSION=\"1.0\"\n";

        assert_eq!(parse_os_release(os_release), LinuxDistro::Unknown);
    }

    /// So is an empty or missing file, which callers pass through as `""`.
    #[test]
    fn no_os_release_at_all_is_unknown() {
        assert_eq!(parse_os_release(""), LinuxDistro::Unknown);
    }

    /// os-release values may be quoted or bare; both are the same value.
    #[test]
    fn quoted_and_bare_ids_are_equivalent() {
        assert_eq!(parse_os_release("ID=\"ubuntu\"\n"), LinuxDistro::Debian);
        assert_eq!(parse_os_release("ID=ubuntu\n"), LinuxDistro::Debian);
        assert_eq!(parse_os_release("ID='ubuntu'\n"), LinuxDistro::Debian);
    }

    #[test]
    fn the_common_families_are_recognised() {
        assert_eq!(parse_os_release("ID=ubuntu\n"), LinuxDistro::Debian);
        assert_eq!(parse_os_release("ID=fedora\n"), LinuxDistro::Fedora);
        assert_eq!(parse_os_release("ID=manjaro\n"), LinuxDistro::Arch);
        assert_eq!(
            parse_os_release("ID=\"opensuse-tumbleweed\"\n"),
            LinuxDistro::Suse
        );
    }

    /// The point of the exercise: `CachyOS` gets a command it can actually run,
    /// for the library whose absence means no system tray at all.
    #[test]
    fn cachyos_is_told_to_use_pacman() {
        assert_eq!(
            install_hint("libappindicator-gtk3", LinuxDistro::Arch).as_deref(),
            Some("pacman -S libayatana-appindicator")
        );
    }

    #[test]
    fn each_family_gets_its_own_installer() {
        assert_eq!(
            install_hint("libssl-dev", LinuxDistro::Debian).as_deref(),
            Some("apt install libssl-dev")
        );
        assert_eq!(
            install_hint("libssl-dev", LinuxDistro::Fedora).as_deref(),
            Some("dnf install openssl-devel")
        );
        assert_eq!(
            install_hint("libssl-dev", LinuxDistro::Suse).as_deref(),
            Some("zypper install libopenssl-devel")
        );
    }

    /// No command is offered for a distribution we could not identify, because
    /// a wrong one gets run and fails where a missing one gets looked up.
    #[test]
    fn an_unknown_distro_gets_no_command() {
        assert!(install_hint("libssl-dev", LinuxDistro::Unknown).is_none());
        assert_eq!(
            packages_for("libssl-dev").map(|p| p.generic),
            Some("OpenSSL development headers")
        );
    }

    /// Dependencies with their own installers must not be dressed up as
    /// distribution packages.
    #[test]
    fn toolchains_that_are_not_system_packages_have_no_row() {
        assert!(packages_for("cargo").is_none());
        assert!(packages_for("rustc").is_none());
        assert!(packages_for("node").is_none());
        assert!(packages_for("npm").is_none());
    }

    /// The compilers share a package, which is why callers de-duplicate.
    #[test]
    fn the_compilers_share_one_package() {
        let names = ["make", "gcc", "g++"].map(|d| {
            packages_for(d)
                .and_then(|p| p.for_distro(LinuxDistro::Arch))
                .expect("toolchain packages are known")
        });

        assert_eq!(names, ["base-devel", "base-devel", "base-devel"]);
    }

    /// Every row must resolve on every family, or some machine gets a hint
    /// with a hole in it.
    #[test]
    fn every_package_resolves_on_every_family() {
        for (dependency, _) in PACKAGES {
            for distro in [
                LinuxDistro::Debian,
                LinuxDistro::Fedora,
                LinuxDistro::Arch,
                LinuxDistro::Suse,
            ] {
                let hint = install_hint(dependency, distro);
                assert!(
                    hint.is_some_and(|h| !h.trim().is_empty()),
                    "{dependency} has no package on {}",
                    distro.label()
                );
            }
        }
    }
}
