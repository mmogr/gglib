//! Device memory for the GPUs `nvidia-smi` and Metal cannot answer for.
//!
//! A `#[path]` child of [`super`] so the probing stays inside the file budget,
//! and because it is one decision rather than a grab-bag: *how many bytes may a
//! model be sized against on this device*. The answer is deliberately allowed
//! to be "we do not know" — see [`other_gpu_memory_bytes`].

use gglib_core::utils::process::cmd;

/// The first physical device `vulkaninfo` reports, reduced to what a budget needs.
///
/// Two facts from one fork. The heap table says how much memory exists; the
/// device type says whether it belongs to the GPU alone. On an integrated GPU
/// the `DEVICE_LOCAL` heap is GTT — host RAM the iGPU may address — so the flag
/// on its own does not mean "VRAM". Measured on a Radeon 840M (RADV KRACKAN1): a 4 GiB
/// carve-out reports an 11.68 GiB device-local heap, 17.52 GiB of heap in
/// total, against 27 GiB of host RAM.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct VulkanDevice {
    /// What `deviceType` said.
    pub(super) kind: DeviceKind,
    /// Largest heap carrying `MEMORY_HEAP_DEVICE_LOCAL_BIT`, in bytes.
    pub(super) device_local_bytes: Option<u64>,
    /// Every heap's size added together, in bytes.
    ///
    /// The ceiling on what an integrated GPU can actually address. amdgpu
    /// reports GTT + VRAM here and the two sum to exactly this figure, so it
    /// is the device's own statement of its limit rather than an inference.
    pub(super) total_heap_bytes: Option<u64>,
}

/// The `deviceType` values that change how a heap should be read.
///
/// Three, not a bool: a software rasteriser reports `DEVICE_LOCAL` heaps like
/// any other device, and they are plain host RAM. Collapsing "not integrated"
/// into "discrete" would hand lavapipe's heap back as if it were VRAM.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum DeviceKind {
    /// `PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU` — heaps are host RAM.
    Integrated,
    /// `PHYSICAL_DEVICE_TYPE_DISCRETE_GPU` — heaps are the card's own VRAM.
    Discrete,
    /// Virtual, CPU, or unrecognised. Not something to size a model against.
    Other,
}

/// The right-hand side of a `key = value` line of `vulkaninfo` output.
fn field_value(rest: &str) -> &str {
    rest.trim_start().trim_start_matches('=').trim()
}

/// Read the first device's type and largest device-local heap from `vulkaninfo`.
///
/// Multi-device hosts print one block per GPU; the first is taken, matching
/// `query_nvidia_memory`'s rule and what llama-server does by default. `None`
/// when no `deviceType` line is present, which is what a missing or failed
/// loader looks like.
pub(super) fn parse_vulkaninfo(stdout: &str) -> Option<VulkanDevice> {
    let mut kind: Option<DeviceKind> = None;
    let (mut largest, mut sum) = (None, None);
    let (mut size, mut local, mut in_heaps) = (None, false, false);

    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("deviceType") {
            // A second device block ends the first. `in_heaps` never resets, so
            // without this a later GPU's heaps would be folded into this one's
            // on any output missing the `memoryTypes:` terminator.
            if kind.is_some() {
                break;
            }
            kind = Some(match field_value(rest) {
                "PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU" => DeviceKind::Integrated,
                "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU" => DeviceKind::Discrete,
                _ => DeviceKind::Other,
            });
        } else if line.starts_with("memoryHeaps:") {
            in_heaps = true;
        } else if in_heaps && line.starts_with("memoryTypes:") {
            break;
        } else if in_heaps && line.starts_with("memoryHeaps[") {
            fold_heap(size, local, &mut largest, &mut sum);
            (size, local) = (None, false);
        } else if in_heaps && let Some(rest) = line.strip_prefix("size") {
            size = field_value(rest)
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok());
        } else if in_heaps && line == "MEMORY_HEAP_DEVICE_LOCAL_BIT" {
            local = true;
        }
    }
    fold_heap(size, local, &mut largest, &mut sum);

    Some(VulkanDevice {
        kind: kind?,
        device_local_bytes: largest,
        total_heap_bytes: sum,
    })
}

/// Fold one finished heap into the running largest-device-local and total.
fn fold_heap(size: Option<u64>, local: bool, largest: &mut Option<u64>, sum: &mut Option<u64>) {
    let Some(bytes) = size else { return };
    if local {
        *largest = (*largest).max(Some(bytes));
    }
    *sum = Some(sum.unwrap_or(0).saturating_add(bytes));
}

/// Parse an amdgpu `mem_info_vram_total` file: one decimal byte count.
///
/// `0` is rejected rather than returned: the kernel reports it for a device
/// whose VRAM could not be sized, and a zero budget reads as a refusal
/// everywhere downstream anyway.
#[cfg(any(target_os = "linux", test))]
fn parse_vram_sysfs(contents: &str) -> Option<u64> {
    contents.trim().parse::<u64>().ok().filter(|&b| b > 0)
}

/// Probe `vulkaninfo` for the first device's type and heaps.
///
/// Forks with no timeout — `utils::process::cmd` is a bare
/// `std::process::Command`. `warm_device_memory` is spawned detached for that
/// reason, but detachment is not a guarantee and this note does not claim it
/// is: `proxy::supervisor::start` reads `total_device_memory_bytes`
/// synchronously a few lines after spawning the warm, and the `OnceLock` means
/// one hung probe blocks every later caller. Measured at 37–42 ms on a healthy
/// host; it is the tail that matters.
fn read_vulkan_device() -> Option<VulkanDevice> {
    let output = cmd("vulkaninfo").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_vulkaninfo(&String::from_utf8_lossy(&output.stdout))
}

/// The amdgpu VRAM carve-out in bytes, from the first card reporting one.
///
/// A file read rather than a fork, so it cannot hang the way `vulkaninfo` can
/// on a wedged driver. It **under-reports on an integrated GPU**, where the
/// carve-out is far smaller than what the device can actually address — that is
/// the conservative direction, and it is only reached when `vulkaninfo` is
/// absent, which is common on headless hosts.
#[cfg(target_os = "linux")]
fn read_amd_vram_bytes() -> Option<u64> {
    let mut paths: Vec<_> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .map(|e| e.path().join("device/mem_info_vram_total"))
        .filter(|p| p.exists())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .find_map(|p| parse_vram_sysfs(&std::fs::read_to_string(p).ok()?))
}

#[cfg(not(target_os = "linux"))]
const fn read_amd_vram_bytes() -> Option<u64> {
    None
}

/// The share of host RAM a unified-memory device may be sized against.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
pub(super) fn unified_share(total_ram_bytes: u64) -> u64 {
    (total_ram_bytes as f64 * super::super::UNIFIED_MEMORY_GPU_SHARE) as u64
}

/// GPU memory for the hosts Metal and `nvidia-smi` cannot answer for.
///
/// An **integrated** GPU is a unified-memory device, and gets the rule Apple
/// Silicon already gets rather than a threshold invented here: its heaps are
/// host RAM, so reporting the largest of them would size a KV cache against
/// system memory — the "working but unusably slow" outcome
/// `total_device_memory_bytes` exists to refuse.
///
/// A **discrete** device's device-local heap is its own VRAM, and is reported
/// as read. Anything else — a virtual or CPU device, a software rasteriser —
/// gets `None`, because a refusal is the honest answer.
///
/// The `bool` is whether the figure describes unified memory, which decides
/// whether the user is shown "VRAM" or "unified memory".
pub(super) fn other_gpu_memory_bytes(total_ram_bytes: u64) -> (Option<u64>, bool) {
    if let Some(device) = read_vulkan_device()
        && let Some(budget) = budget_for(&device, total_ram_bytes)
    {
        return (Some(budget), device.kind == DeviceKind::Integrated);
    }
    // The sysfs carve-out is a discrete-style figure even on an iGPU, so it is
    // never reported as unified: under-stating the budget is the safe error,
    // over-stating what it describes is not.
    (read_amd_vram_bytes(), false)
}

/// The decision, over a plain struct.
///
/// Lifted out of the probe for the same reason `device_budget_of` is: the
/// interesting part is one branch of policy, and it should be assertable
/// without the hardware it describes.
fn budget_for(device: &VulkanDevice, total_ram_bytes: u64) -> Option<u64> {
    match device.kind {
        // Capped by what the device says it has. Apple's 0.75 is backed by a
        // platform limit; amdgpu's GTT ceiling is a kernel default — ~50% of
        // RAM on the measured host — and it is *reported*, so the share must
        // not promise past the heaps. Uncapped, this host was handed 20.28 GiB
        // against 17.52 GiB of total heap, and a model landing between the two
        // would pass the fit and then fail to allocate.
        DeviceKind::Integrated => {
            let share = unified_share(total_ram_bytes);
            Some(device.total_heap_bytes.map_or(share, |h| share.min(h)))
        }
        DeviceKind::Discrete => device.device_local_bytes,
        DeviceKind::Other => None,
    }
}

#[cfg(test)]
#[path = "device_memory_tests.rs"]
mod device_memory_tests;
