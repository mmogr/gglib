#![doc = include_str!("README.md")]
mod deps;
mod fit_memory;
pub(crate) mod glibc;
pub(crate) mod gpu;
mod host;
mod suggest;
pub(crate) mod vulkan_devices;

use gglib_core::host::HostOs;
use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{
    BuildOrigin, DependencyReport, EngineInstall, Found, GpuInfo, LinuxDistro, SystemMemoryInfo,
    dependency_report, needs_on, parse_os_release,
};

pub(crate) use fit_memory::fit_memory_of;
pub use fit_memory::{FitMemory, a_gpu_is_present};
use gpu::{detect_gpu_info, get_system_memory_info};
pub use host::{free_disk_bytes, host_facts, this_machine_name};
pub use suggest::{
    UNREADABLE_GPU, download_budget, download_budget_on, launching, no_suggestion_reason,
    no_suggestion_reason_on, recommend_here, recommend_on, recommend_with, sized_against,
    sized_against_on,
};

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
/// let report = probe.dependency_report();
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
    // Read once: the figure does not change while the process runs, and the
    // probe behind it builds a whole `System` and asks after the GPU each
    // time. A plan of every launch (`gglib model explain`, the inspector)
    // asks it as a launch does.
    static TOTAL: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *TOTAL.get_or_init(|| get_system_memory_info().total_ram_bytes)
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
    /// Every other machine. Unlike `total_device_memory_bytes`, the *free*
    /// reading has no Vulkan path, so an AMD or Intel host reports nothing.
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
/// model resident, which is always safe.
///
/// Cached for [`FREE_VRAM_TTL`], so a burst of admissions costs one probe.
#[must_use]
pub fn free_gpu_memory_bytes() -> Option<u64> {
    free_gpu_memory_bytes_within(FREE_VRAM_TTL)
}

/// [`free_gpu_memory_bytes`] for a reader content with a reading up to
/// `max_age` old.
///
/// The dashboard's "room for one more" is drawn every second for the life of
/// the proxy and must not fork `nvidia-smi` each time. A reading younger than
/// `max_age` is answered from the cache; an older one is probed anew, and the
/// probe refreshes the cache for every reader.
#[must_use]
pub fn free_gpu_memory_bytes_within(max_age: std::time::Duration) -> Option<u64> {
    let now = std::time::Instant::now();

    {
        let guard = FREE_VRAM_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((probed_at, value)) = *guard
            && now.duration_since(probed_at) < max_age
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

    *FREE_VRAM_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((now, value));
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
        .map_or(LinuxDistro::Unknown, |contents| parse_os_release(&contents))
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

/// How llama.cpp's next install here would be made, with nobody asking for
/// a build: the engine's own plan, as `gglib config llama install` and the
/// daemon's install route read it. Where its install cannot be located, only
/// whether this machine has a download is left to go on.
fn llama_install() -> EngineInstall {
    use crate::engine::{InstallPlan, download, pin_state, plan_install};
    use gglib_core::domain::RuntimeKind;

    let Ok(state) = pin_state(RuntimeKind::Llama) else {
        return if download(RuntimeKind::Llama).is_ok() {
            EngineInstall::Download
        } else {
            EngineInstall::NoDownload
        };
    };
    match plan_install(RuntimeKind::Llama, &state, false, false) {
        InstallPlan::Download { .. } => EngineInstall::Download,
        InstallPlan::Build {
            no_download: Some(_),
        } => EngineInstall::NoDownload,
        InstallPlan::Build { no_download: None } => EngineInstall::Rebuild,
    }
}

impl SystemProbePort for DefaultSystemProbe {
    /// Each row of the dependency table is looked for under its names on
    /// this OS, and the report is made from what answered.
    ///
    /// This is an installed copy unless the checkout it was built from is
    /// still here ([`gglib_core::paths::source_checkout`]).
    fn dependency_report(&self) -> DependencyReport {
        let os = HostOs::current();
        let found = Found {
            versions: needs_on(os)
                .filter_map(|need| deps::version_of(need, os).map(|version| (need.name, version)))
                .collect(),
            gpu: self.detect_gpu_info(),
            llama_installed: crate::llama::check_llama_installed(),
            sd_installed: gglib_core::paths::sd_server_path().is_ok_and(|path| path.is_file()),
            llama_install: llama_install(),
        };
        let origin = if gglib_core::paths::source_checkout().is_some() {
            BuildOrigin::Checkout
        } else {
            BuildOrigin::Installed
        };
        dependency_report(os, origin, detect_linux_distro(), &found)
    }

    fn detect_gpu_info(&self) -> GpuInfo {
        detect_gpu_info()
    }

    fn get_system_memory_info(&self) -> SystemMemoryInfo {
        get_system_memory_info()
    }
}

/// What a context fit on this machine spends against, cached: the one
/// answer every launch, every plan of one (`gglib model explain`, the
/// inspector) and the endpoint's advertisement read.
///
/// A GPU's nominal capacity (unified memory on Apple and on an integrated
/// GPU, VRAM on a discrete card), never a live free reading. Free memory
/// moves with whatever else the user has open, and a budget that moves
/// produces a fitted context that moves, which recycles the resident and
/// blows its prefix cache. The caller reserves a fixed allowance for the
/// second slot from this figure and must *not* net out the models actually
/// loaded: that set changes while a model is resident, which is exactly the
/// drift this avoids.
///
/// A machine with no GPU runs its model from RAM and is sized against a
/// share of it ([`FitMemory::SystemRam`], see `fit_memory_of`).
///
/// No budget ([`FitMemory::Unreadable`]) when gglib cannot read a GPU's
/// memory: a device whose heaps cannot be shown to be its own, or an NVIDIA
/// card `nvidia-smi` cannot answer for. Deliberately a refusal rather than a
/// fallback to system RAM:
/// sizing a KV cache against 64 GiB of host memory on an 8 GiB card is
/// exactly the "working but unusably slow" outcome the recommendation module
/// exists to prevent, and it would look stable while measuring the wrong
/// device.
///
/// Cached in a `OnceLock` because the probe behind it builds a whole
/// `System` snapshot and forks `nvidia-smi` and `vulkaninfo`; this is read
/// from the admission path. Warm it at startup — see `warm_device_memory`.
#[must_use]
pub fn fit_memory() -> FitMemory {
    let machine = detected();
    fit_memory_of(machine.gpu_present, &machine.memory)
}

/// What was detected of this machine: whether it has a GPU an engine runs
/// on, and its memory.
struct Detected {
    gpu_present: bool,
    memory: SystemMemoryInfo,
}

/// [`Detected`], probed once and kept: every answer sized to this machine
/// ([`fit_memory()`], [`download_budget`], [`recommend_here`]) reads the one
/// detection.
fn detected() -> &'static Detected {
    static MACHINE: std::sync::OnceLock<Detected> = std::sync::OnceLock::new();
    MACHINE.get_or_init(|| {
        let gpu = detect_gpu_info();
        Detected {
            gpu_present: fit_memory::a_gpu_is_present(&gpu),
            memory: gpu::memory_info_for(&gpu),
        }
    })
}

/// The GPU's memory, when [`fit_memory()`] read one: `None` on a machine with
/// no GPU and on one whose GPU could not be read.
#[must_use]
pub fn total_device_memory_bytes() -> Option<u64> {
    fit_memory().device_bytes()
}

/// Populate the [`total_device_memory_bytes`] cache off the async runtime.
///
/// The first call forks `nvidia-smi` and enumerates every process; doing that
/// lazily from `admit` would block a tokio worker on the first request.
pub async fn warm_device_memory() {
    let _ = tokio::task::spawn_blocking(fit_memory).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_system_probe_creation() {
        let probe = DefaultSystemProbe::new();
        // Just verify it can be created
        let _report = probe.dependency_report();
    }

    #[test]
    fn test_default_system_probe_default_trait() {
        let probe = DefaultSystemProbe;
        let _report = probe.dependency_report();
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        expect(
            clippy::used_underscore_binding,
            reason = "read only on macOS; the underscore keeps it unused elsewhere without a warning"
        )
    )]
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

    /// The free-VRAM probe runs on every admission that considers a co-load, so
    /// it has to be safe on hardware it knows nothing about. `None` is a real
    /// answer here — it means gglib keeps one model resident, which is always
    /// correct — and a panic or a hang would take the request path with it.
    /// So is 0: a GPU another program has filled has nothing free, and a
    /// co-load refused then is the right answer.
    #[test]
    fn free_gpu_memory_is_answerable_on_any_machine() {
        let first = free_gpu_memory_bytes();

        // A reading, if there is one, is of the device the total describes:
        // never more than it holds, as a wrapped figure would be.
        if let (Some(free), Some(total)) = (first, total_device_memory_bytes()) {
            assert!(free <= total, "{free} free of a {total}-byte device");
        }

        // Cached for `FREE_VRAM_TTL`, so a burst of admissions costs one probe.
        // Two calls this close together must agree by construction.
        assert_eq!(first, free_gpu_memory_bytes());
    }

    /// A machine that cannot report free VRAM must say so rather than guessing,
    /// because a guess in the optimistic direction co-loads a model that then
    /// OOMs the primary mid-generation. CPU-only and Vulkan-only hosts — this
    /// CI runner among them — take this path.
    #[test]
    fn an_unreadable_budget_reports_nothing_rather_than_a_guess() {
        let source = detect_free_vram_source();
        if source == FreeVramSource::Unavailable {
            assert_eq!(free_gpu_memory_bytes(), None);
        }
    }
}
