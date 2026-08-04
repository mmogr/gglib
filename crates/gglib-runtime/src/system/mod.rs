#![doc = include_str!("README.md")]
mod commands;
mod deps;
pub(crate) mod gpu;

use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{
    Dependency, DependencyStatus, GpuInfo, LinuxDistro, SystemMemoryInfo, install_hint,
    packages_for, parse_os_release,
};

#[cfg(target_os = "linux")]
use commands::get_patchelf_version;
use commands::{
    get_cargo_version, get_cmake_version, get_gcc_version, get_git_version, get_gxx_version,
    get_make_version, get_node_version, get_npm_version, get_pkgconfig_version,
    get_python3_version, get_rustc_version,
};
use deps::check_libssl;
#[cfg(target_os = "linux")]
use deps::{
    check_libappindicator, check_libasound, check_libclang, check_libcurl, check_librsvg,
    check_libsqlite3, check_webkit2gtk,
};
use gpu::{detect_gpu_info, get_system_memory_info};

/// Default implementation of `SystemProbePort`.
///
/// This struct provides active system probing by executing commands
/// and querying hardware. It should be constructed in CLI's main.rs
/// and passed to handlers that need system information.
///
/// # Example
///
/// ```ignore
/// use gglib_runtime::system::DefaultSystemProbe;
/// use gglib_core::ports::SystemProbePort;
///
/// let probe = DefaultSystemProbe::new();
/// let deps = probe.check_all_dependencies();
/// ```
/// Total physical system RAM in bytes.
///
/// A direct accessor for the one figure the launch path needs to size
/// llama-server's host-RAM prompt cache, without constructing or threading a
/// full [`SystemProbePort`] through `ProcessManager`. Returns `0` if the
/// platform query fails, which callers treat as "unknown".
///
/// Public (not `pub(crate)`) so launch surfaces outside this crate that need
/// the same cache-RAM auto-sizing math (e.g. `gglib-app-services`' direct
/// model-serve path) can call it without going through `ProcessManager`.
pub fn total_system_ram_bytes() -> u64 {
    get_system_memory_info().total_ram_bytes
}

/// Parse a string as a truthy on/off flag (case- and whitespace-insensitive).
///
/// Used by `GGLIB_DISABLE_<FEATURE>` environment variable checks throughout
/// the crate. Truthy values: `1`, `true`, `yes`, `on`.
pub(crate) fn is_truthy_flag(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Identify the running distribution from `/etc/os-release`.
///
/// The I/O half of [`gglib_core::utils::system::parse_os_release`]: this layer
/// reads the file, the domain layer decides what it means. A file that is
/// missing or unreadable — which includes every non-Linux host — yields
/// [`LinuxDistro::Unknown`], the same answer as one that is simply not
/// recognised, because callers treat both the same way.
#[must_use]
pub fn detect_linux_distro() -> LinuxDistro {
    std::fs::read_to_string("/etc/os-release")
        .map(|contents| parse_os_release(&contents))
        .unwrap_or(LinuxDistro::Unknown)
}

/// The install hint for a dependency on this machine.
///
/// Falls back to prose when there is no command to give — an unidentified
/// distribution, or a dependency like `cargo` that does not come from the
/// system package manager at all. Prose is deliberately preferred over a
/// plausible-looking guess: a wrong command gets run and fails, whereas
/// "OpenSSL development headers" gets looked up.
fn hint_for(dependency: &str, distro: LinuxDistro) -> String {
    install_hint(dependency, distro).unwrap_or_else(|| {
        packages_for(dependency).map_or_else(
            || format!("install {dependency}"),
            |names| format!("install {}", names.generic),
        )
    })
}

/// A dependency installed through the system package manager.
///
/// Collapses the builder chain that was repeated for all twenty-odd entries
/// below, so a dependency is one line and its install hint comes from the
/// shared package table rather than a hardcoded `apt` string.
fn system_dep(
    name: &'static str,
    description: &'static str,
    distro: LinuxDistro,
    version: Option<String>,
) -> Dependency {
    Dependency::required(name, description)
        .with_hint(hint_for(name, distro))
        .with_status(version.map_or(DependencyStatus::Missing, |version| {
            DependencyStatus::Present { version }
        }))
}

/// A dependency with its own installer, pointed at rather than packaged.
fn hosted_dep(
    name: &'static str,
    description: &'static str,
    url: &'static str,
    version: Option<String>,
) -> Dependency {
    Dependency::required(name, description)
        .with_hint(url)
        .with_status(version.map_or(DependencyStatus::Missing, |version| {
            DependencyStatus::Present { version }
        }))
}

/// A dependency detected by probe rather than by version string.
fn probed_dep(
    name: &'static str,
    description: &'static str,
    distro: LinuxDistro,
    present: bool,
) -> Dependency {
    system_dep(
        name,
        description,
        distro,
        present.then(|| "available".to_owned()),
    )
}

pub struct DefaultSystemProbe;

impl DefaultSystemProbe {
    /// Create a new default system probe.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultSystemProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProbePort for DefaultSystemProbe {
    fn check_all_dependencies(&self) -> Vec<Dependency> {
        let gpu_info = self.detect_gpu_info();

        // Read once and thread it through: every install hint below keys off
        // the same answer, and re-reading the file per dependency would be
        // twenty syscalls to learn the same thing.
        let distro = detect_linux_distro();

        let mut deps = vec![
            // Core Rust toolchain (required)
            hosted_dep(
                "cargo",
                "Required for building Rust code",
                "https://rustup.rs",
                get_cargo_version(),
            ),
            hosted_dep(
                "rustc",
                "Rust compiler",
                "https://rustup.rs",
                get_rustc_version(),
            ),
            // Node.js ecosystem (required for GUI)
            hosted_dep(
                "node",
                "Required for building web UI and Tauri",
                "https://nodejs.org",
                get_node_version(),
            ),
            hosted_dep(
                "npm",
                "Node package manager",
                "https://nodejs.org",
                get_npm_version(),
            ),
            // Build tools (required)
            system_dep(
                "git",
                "Required for llama.cpp installation",
                distro,
                get_git_version(),
            ),
            system_dep(
                "make",
                "Required for llama.cpp build",
                distro,
                get_make_version(),
            ),
            system_dep(
                "gcc",
                "Required for llama.cpp compilation",
                distro,
                get_gcc_version(),
            ),
            system_dep(
                "g++",
                "Required for llama.cpp compilation",
                distro,
                get_gxx_version(),
            ),
            system_dep(
                "pkg-config",
                "Required for building with system libraries",
                distro,
                get_pkgconfig_version(),
            ),
            system_dep(
                "libssl-dev",
                "Required for HTTPS support",
                distro,
                check_libssl(),
            ),
            system_dep(
                "cmake",
                "Required for llama.cpp build",
                distro,
                get_cmake_version(),
            ),
            system_dep(
                "python3",
                "Required for hf_xet fast download helper",
                distro,
                get_python3_version(),
            ),
        ];

        // Add GTK/Tauri dependencies for Linux only
        #[cfg(target_os = "linux")]
        {
            deps.extend(vec![
                system_dep(
                    "patchelf",
                    "Required for Tauri AppImage bundling",
                    distro,
                    get_patchelf_version(),
                ),
                system_dep(
                    "webkit2gtk-4.1",
                    "Required for Tauri desktop app (WebView)",
                    distro,
                    check_webkit2gtk(),
                ),
                system_dep(
                    "librsvg",
                    "Required for Tauri desktop app (SVG rendering)",
                    distro,
                    check_librsvg(),
                ),
                system_dep(
                    "libappindicator-gtk3",
                    "Required for Tauri system tray support",
                    distro,
                    check_libappindicator(),
                ),
                system_dep(
                    "libasound2-dev",
                    "Required for voice/audio support",
                    distro,
                    check_libasound(),
                ),
                system_dep(
                    "libcurl-dev",
                    "Required for llama.cpp HTTP/HTTPS support",
                    distro,
                    check_libcurl(),
                ),
                system_dep(
                    "libsqlite3-dev",
                    "Required for database support",
                    distro,
                    check_libsqlite3(),
                ),
                system_dep(
                    "libclang-dev",
                    "Required for Rust FFI bindings (bindgen)",
                    distro,
                    check_libclang(),
                ),
            ]);
        }

        // Add GPU acceleration info based on detection
        if gpu_info.has_metal {
            deps.push(
                Dependency::optional("Metal", "Apple GPU acceleration (built-in)").with_status(
                    DependencyStatus::Present {
                        version: "available".to_string(),
                    },
                ),
            );
        } else if let Some(cuda_version) = gpu_info.cuda_version.clone() {
            deps.push(
                Dependency::optional("CUDA", "NVIDIA GPU acceleration for faster inference")
                    .with_hint("https://developer.nvidia.com/cuda-downloads")
                    .with_status(DependencyStatus::Present {
                        version: cuda_version,
                    }),
            );
        } else if gpu_info.has_nvidia_gpu {
            // GPU hardware present but CUDA not installed
            deps.push(
                Dependency::optional(
                    "CUDA",
                    "NVIDIA GPU detected - install CUDA for GPU acceleration",
                )
                .with_hint("https://developer.nvidia.com/cuda-downloads")
                .with_status(DependencyStatus::Optional),
            );
        }

        if gpu_info.has_vulkan {
            deps.push(
                Dependency::optional(
                    "Vulkan runtime",
                    "GPU acceleration via Vulkan (AMD, Intel, NVIDIA)",
                )
                .with_status(DependencyStatus::Present {
                    version: "available".to_string(),
                }),
            );

            // When the runtime is present, the build-time deps below
            // are *required* — auto-detect picks Vulkan and the build
            // would fail without them, so flag them as hard misses
            // rather than silently degrading to a CPU build.
            deps.push(probed_dep(
                "Vulkan headers",
                "Development headers required to build with -DGGML_VULKAN=ON",
                distro,
                gpu_info.vulkan_headers,
            ));

            deps.push(probed_dep(
                "glslc",
                "SPIR-V shader compiler required for Vulkan builds",
                distro,
                gpu_info.vulkan_glslc,
            ));

            deps.push(probed_dep(
                "SPIR-V headers",
                "spirv/unified1/spirv.hpp required to build with -DGGML_VULKAN=ON",
                distro,
                gpu_info.vulkan_spirv_headers,
            ));
        } else if !gpu_info.has_metal {
            // Only suggest Vulkan on non-macOS (macOS uses Metal)
            #[cfg(not(target_os = "macos"))]
            deps.push(
                Dependency::optional("Vulkan", "Install Vulkan drivers for GPU acceleration")
                    .with_hint(hint_for("Vulkan", distro))
                    .with_status(DependencyStatus::Optional),
            );
        }

        deps
    }

    fn detect_gpu_info(&self) -> GpuInfo {
        detect_gpu_info()
    }

    fn get_system_memory_info(&self) -> SystemMemoryInfo {
        get_system_memory_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_system_probe_creation() {
        let probe = DefaultSystemProbe::new();
        // Just verify it can be created
        let _deps = probe.check_all_dependencies();
    }

    #[test]
    fn test_default_system_probe_default_trait() {
        let probe = DefaultSystemProbe;
        let _deps = probe.check_all_dependencies();
    }

    #[test]
    fn test_gpu_detection() {
        let probe = DefaultSystemProbe::new();
        let _gpu = probe.detect_gpu_info();
        // On macOS, should have Metal
        #[cfg(target_os = "macos")]
        assert!(_gpu.has_metal);
    }

    #[test]
    fn test_memory_info() {
        let probe = DefaultSystemProbe::new();
        let mem = probe.get_system_memory_info();
        // RAM should always be > 1GB
        assert!(mem.total_ram_bytes > 1_000_000_000);
    }
}
