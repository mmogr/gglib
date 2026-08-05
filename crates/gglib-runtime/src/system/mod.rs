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
    check_gtk_layer_shell, check_libappindicator, check_libasound, check_libclang, check_libcurl,
    check_librsvg, check_libsqlite3, check_webkit2gtk,
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

/// How long a free-VRAM reading is reused before the probe runs again.
///
/// Every reading costs a `nvidia-smi` fork (~20–50 ms), and admission can ask
/// several times a second under load. Two seconds is short enough that the
/// figure still reflects a model that finished loading moments ago, and long
/// enough that a burst of requests pays for one probe rather than one each.
const FREE_VRAM_TTL: std::time::Duration = std::time::Duration::from_secs(2);

/// Where this machine's free-GPU-memory reading comes from.
///
/// Resolved once and cached: the underlying detection shells out to
/// `nvidia-smi`/`lspci`, which is far too expensive to repeat per admission,
/// and the answer cannot change without a reboot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FreeVramSource {
    /// Discrete NVIDIA GPU — `nvidia-smi --query-gpu=memory.free`.
    NvidiaSmi,
    /// Apple Silicon unified memory — free host RAM, scaled by the same share
    /// [`gpu::get_system_memory_info`] treats as GPU-addressable.
    UnifiedMemory,
    /// Every other machine. gglib reads VRAM for Metal and NVIDIA only, so an
    /// AMD, Intel, or CPU-only host reports nothing rather than guessing.
    Unavailable,
}

static FREE_VRAM_SOURCE: std::sync::OnceLock<FreeVramSource> = std::sync::OnceLock::new();
static FREE_VRAM_CACHE: std::sync::Mutex<Option<(std::time::Instant, Option<u64>)>> =
    std::sync::Mutex::new(None);

/// Share of free host RAM treated as GPU-addressable on unified-memory Macs.
///
/// The same 75% [`gpu::get_system_memory_info`] applies to *total* RAM when
/// reporting Apple Silicon's GPU budget — applied here to the *free* figure so
/// the two describe the same pool.
const UNIFIED_MEMORY_GPU_SHARE: f64 = 0.75;

/// Free GPU memory in bytes, right now, or `None` when this machine cannot
/// report it.
///
/// This is the live figure the second-resident-slot decision is made against
/// (see [`gglib_core::domain::decide_secondary_slot`]) — what is actually free
/// with the primary model already loaded, not the card's nominal capacity.
///
/// `None` is a real answer, not a failure: it means gglib will keep exactly one
/// model resident, which is the pre-M9 behaviour and always safe.
///
/// Cached for [`FREE_VRAM_TTL`], so a burst of admissions costs one probe.
#[must_use]
pub fn free_gpu_memory_bytes() -> Option<u64> {
    let now = std::time::Instant::now();

    {
        let guard = FREE_VRAM_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((probed_at, value)) = *guard
            && now.duration_since(probed_at) < FREE_VRAM_TTL
        {
            return value;
        }
    }

    // Probed outside the lock: the probe forks a process, and holding the
    // mutex across it would serialise every concurrent admission behind it.
    // A racing probe is harmless — both compute the same answer and the later
    // write wins.
    let source = *FREE_VRAM_SOURCE.get_or_init(detect_free_vram_source);
    let value = match source {
        FreeVramSource::NvidiaSmi => gpu::get_nvidia_free_vram_bytes(),
        FreeVramSource::UnifiedMemory => {
            #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
            #[allow(clippy::cast_possible_truncation)]
            let free = (gpu::get_available_ram_bytes() as f64 * UNIFIED_MEMORY_GPU_SHARE) as u64;
            Some(free)
        }
        FreeVramSource::Unavailable => None,
    };

    *FREE_VRAM_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, value));
    value
}

/// Decide once how this machine reports free GPU memory.
fn detect_free_vram_source() -> FreeVramSource {
    let info = gpu::detect_gpu_info();
    if info.has_metal {
        FreeVramSource::UnifiedMemory
    } else if info.has_nvidia_gpu && gpu::get_nvidia_free_vram_bytes().is_some() {
        FreeVramSource::NvidiaSmi
    } else {
        FreeVramSource::Unavailable
    }
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

/// A dependency whose absence degrades a feature rather than breaking the build.
///
/// Reported so `check-deps` can name it and give the right install command, but
/// marked optional so a missing one is not a failure.
fn optional_dep(
    name: &'static str,
    description: &'static str,
    distro: LinuxDistro,
    version: Option<String>,
) -> Dependency {
    Dependency::optional(name, description)
        .with_hint(hint_for(name, distro))
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
            // Optional, not required: downloads run natively over HTTP. Python
            // only enables the hf_xet accelerator, and only if the user opts in
            // to provisioning it.
            optional_dep(
                "python3",
                "Optional: enables the hf_xet download accelerator",
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
                optional_dep(
                    "gtk-layer-shell",
                    "Anchors the tray panel beside the system tray on Wayland",
                    distro,
                    check_gtk_layer_shell(),
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
