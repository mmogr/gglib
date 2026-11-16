//! Which Vulkan devices this machine has, and whether one of them is a GPU a
//! Vulkan build of an engine can run on.
//!
//! The one owner of that question: every report of a Vulkan GPU and every
//! choice between an engine's Vulkan and CPU builds reads
//! [`vulkan_gpu_present`], and only [`vulkan_devices`] asks the machine.
//!
//! The Vulkan loader is not enough: Windows installs it with no GPU driver
//! at all, and a Vulkan build there runs on the CPU and says nothing.

#[cfg(any(target_os = "linux", target_os = "windows"))]
use crate::llama::detect::tools::command_stdout;
#[cfg(target_os = "windows")]
use crate::llama::detect::tools::command_succeeds;

/// One device as the Vulkan driver lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VulkanDevice {
    /// Its `deviceType`, as printed: `PHYSICAL_DEVICE_TYPE_DISCRETE_GPU`.
    pub(crate) device_type: String,
    /// Its `deviceName`: `AMD Radeon RX 7900 XTX (RADV NAVI31)`.
    pub(crate) name: String,
}

/// The device types a GPU reports. A virtual GPU is a host's GPU passed to
/// a virtual machine; a CPU device is a software rasteriser.
const GPU_TYPES: [&str; 3] = [
    "PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU",
    "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU",
    "PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU",
];

/// Software rasterisers, by a word of their device names, for one that
/// reports a GPU's type: Mesa's (llvmpipe, lavapipe), Google's
/// (`SwiftShader`), and Windows' own, which Mesa's Dozen lists.
const SOFTWARE_RENDERERS: [&str; 4] =
    ["llvmpipe", "lavapipe", "swiftshader", "basic render driver"];

impl VulkanDevice {
    /// Whether this device is a GPU: a GPU's type, and not a software
    /// rasteriser's name.
    fn is_a_gpu(&self) -> bool {
        let device_type = self.device_type.trim_start_matches("VK_");
        let name = self.name.to_lowercase();
        GPU_TYPES.contains(&device_type)
            && !SOFTWARE_RENDERERS.iter().any(|word| name.contains(word))
    }
}

/// What was found out about this machine's Vulkan devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VulkanDevices {
    /// The devices the Vulkan driver lists.
    Listed(Vec<VulkanDevice>),
    /// No list could be read; whether the operating system shows a GPU with
    /// a Vulkan driver installed for it.
    Unlisted {
        /// A GPU and its Vulkan driver are installed.
        gpu_driver: bool,
    },
}

/// Whether a Vulkan build runs on a GPU here, from the operating system `os`
/// (a `std::env::consts::OS` spelling), whether the Vulkan loader is there,
/// and the `devices` found: a listed device that is a GPU, or, with no list,
/// a GPU with its Vulkan driver. Never on macOS, whose one build is Metal.
pub(crate) fn vulkan_gpu_present(os: &str, has_loader: bool, devices: &VulkanDevices) -> bool {
    let gpu = match devices {
        VulkanDevices::Listed(listed) => listed.iter().any(VulkanDevice::is_a_gpu),
        VulkanDevices::Unlisted { gpu_driver } => *gpu_driver,
    };
    os != "macos" && has_loader && gpu
}

/// This machine's Vulkan devices: the list `vulkaninfo --summary` prints,
/// where it runs and lists one. Else what the operating system shows:
///
/// - Linux: a GPU device, a DRM render node (`/dev/dri/renderD*`, which an
///   AMD, Intel or NVIDIA driver makes), NVIDIA's own device node, or WSL2's
///   GPU (`/dev/dxg`); and a Vulkan driver for it, an ICD manifest, since
///   the loader library is often installed by other packages with no driver
///   at all.
/// - Windows: a display adapter with a Vulkan driver registered
///   (`VulkanDriverName` in its registry key, as GPU drivers write it), or a
///   driver in the Khronos list older drivers use. Microsoft's Basic Display
///   Adapter has neither.
///
/// macOS lists none and is not asked: its build is Metal.
pub(crate) fn vulkan_devices() -> VulkanDevices {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let listed = command_stdout("vulkaninfo", &["--summary"])
            .map(|summary| readers::parse_summary(&summary))
            .unwrap_or_default();
        listed_or(listed, gpu_driver_installed)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    listed_or(Vec::new(), || false)
}

/// The devices `listed`, when the list holds one; else whether the
/// operating system shows a GPU with its Vulkan driver, asked of
/// `gpu_driver`. A list that holds nothing is no answer: `vulkaninfo` fails
/// when there is no driver, so an empty list is output the parser did not
/// know.
fn listed_or(listed: Vec<VulkanDevice>, gpu_driver: impl FnOnce() -> bool) -> VulkanDevices {
    if listed.is_empty() {
        VulkanDevices::Unlisted {
            gpu_driver: gpu_driver(),
        }
    } else {
        VulkanDevices::Listed(listed)
    }
}

/// Whether Linux shows a GPU device and a Vulkan driver for it.
#[cfg(target_os = "linux")]
fn gpu_driver_installed() -> bool {
    use std::path::Path;
    readers::gpu_device_under(Path::new("/dev"))
        && readers::vulkan_driver_under(&[
            Path::new("/usr/share/vulkan/icd.d"),
            Path::new("/etc/vulkan/icd.d"),
        ])
}

/// Whether Windows shows a display adapter with a Vulkan driver, or a
/// driver in the Khronos list.
#[cfg(target_os = "windows")]
fn gpu_driver_installed() -> bool {
    /// The display adapters' class: one numbered subkey per adapter, where
    /// its driver registers a Vulkan driver.
    const DISPLAY_ADAPTERS: &str =
        r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    /// Vulkan drivers by their manifests, as drivers older than the
    /// per-adapter entry list them.
    const KHRONOS_DRIVERS: &str = r"HKLM\SOFTWARE\Khronos\Vulkan\Drivers";

    // A search of the whole class answers by its exit code alone, which no
    // code page can garble; asked key by key too, in case a key the user may
    // not read fails the search.
    let searched = command_succeeds(
        "reg",
        &["query", DISPLAY_ADAPTERS, "/s", "/v", "VulkanDriverName"],
    );
    let registered = || {
        command_stdout("reg", &["query", DISPLAY_ADAPTERS]).is_some_and(|listing| {
            readers::adapter_keys(&listing)
                .any(|key| command_succeeds("reg", &["query", key, "/v", "VulkanDriverName"]))
        })
    };
    searched
        || registered()
        || command_stdout("reg", &["query", KHRONOS_DRIVERS])
            .is_some_and(|drivers| readers::names_a_driver(&drivers))
}

/// What each operating system's answer means, read from its output or its
/// files: plain functions, so every one is tested on every OS.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "each OS calls its own readers (Linux's files, Windows' registry, vulkaninfo on both)"
    )
)]
pub(super) mod readers {
    use super::VulkanDevice;

    /// The devices in `vulkaninfo --summary`'s output: each `deviceType`
    /// line starts one, and the `deviceName` after it names it.
    pub(crate) fn parse_summary(summary: &str) -> Vec<VulkanDevice> {
        let mut devices: Vec<VulkanDevice> = Vec::new();
        for line in summary.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().to_owned();
            match key.trim() {
                "deviceType" => devices.push(VulkanDevice {
                    device_type: value,
                    name: String::new(),
                }),
                "deviceName" => {
                    if let Some(device) = devices.last_mut() {
                        device.name = value;
                    }
                }
                _ => {}
            }
        }
        devices
    }

    /// The adapters' keys in `reg query`'s listing of the display adapters'
    /// class: the subkeys named by a number (`…\0000`), not `Configuration`
    /// or `Properties`.
    pub(crate) fn adapter_keys(listing: &str) -> impl Iterator<Item = &str> {
        listing.lines().map(str::trim).filter(|line| {
            line.starts_with("HKEY_")
                && line.rsplit_once('\\').is_some_and(|(_, last)| {
                    !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit())
                })
        })
    }

    /// Whether `reg query`'s listing of [`KHRONOS_DRIVERS`] holds a driver:
    /// a value named by a manifest's path.
    pub(crate) fn names_a_driver(listing: &str) -> bool {
        listing
            .lines()
            .any(|line| line.to_ascii_lowercase().contains(".json"))
    }

    /// Whether one of the ICD directories `dirs` holds a Vulkan driver's
    /// manifest, a `.json` file.
    pub(crate) fn vulkan_driver_under(dirs: &[&std::path::Path]) -> bool {
        dirs.iter().any(|dir| {
            std::fs::read_dir(dir).is_ok_and(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            })
        })
    }

    /// Whether the device directory `dev` holds a GPU's device node: a DRM
    /// render node, NVIDIA's, or WSL2's.
    pub(crate) fn gpu_device_under(dev: &std::path::Path) -> bool {
        let render_node = std::fs::read_dir(dev.join("dri")).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
        });
        render_node || dev.join("nvidia0").exists() || dev.join("dxg").exists()
    }
}

/// The machines the choice is tested against, by what each one's Vulkan
/// driver lists or its operating system shows.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::{VulkanDevice, VulkanDevices};

    fn listed(devices: &[(&str, &str)]) -> VulkanDevices {
        VulkanDevices::Listed(
            devices
                .iter()
                .map(|&(device_type, name)| VulkanDevice {
                    device_type: device_type.to_owned(),
                    name: name.to_owned(),
                })
                .collect(),
        )
    }

    /// Machines where no Vulkan device is a GPU, and what each one has.
    pub(crate) fn without_a_gpu() -> Vec<(&'static str, VulkanDevices)> {
        vec![
            ("no device listed", listed(&[])),
            (
                "llvmpipe",
                listed(&[(
                    "PHYSICAL_DEVICE_TYPE_CPU",
                    "llvmpipe (LLVM 17.0.6, 256 bits)",
                )]),
            ),
            (
                "SwiftShader",
                listed(&[(
                    "PHYSICAL_DEVICE_TYPE_CPU",
                    "SwiftShader Device (Subzero) (0x0000C0DE)",
                )]),
            ),
            // Windows' software renderer under Mesa's Dozen. Its type is not
            // relied on, so it is given a GPU's here.
            (
                "the Microsoft Basic Render Driver",
                listed(&[(
                    "PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU",
                    "Microsoft Direct3D12 (Microsoft Basic Render Driver)",
                )]),
            ),
            (
                "no list and no GPU driver",
                VulkanDevices::Unlisted { gpu_driver: false },
            ),
        ]
    }

    /// Machines with a GPU a Vulkan build runs on, and how it was found.
    pub(crate) fn with_a_gpu() -> Vec<(&'static str, VulkanDevices)> {
        vec![
            (
                "a discrete GPU",
                listed(&[(
                    "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU",
                    "AMD Radeon RX 7900 XTX (RADV NAVI31)",
                )]),
            ),
            (
                "an integrated GPU listed after llvmpipe",
                listed(&[
                    (
                        "PHYSICAL_DEVICE_TYPE_CPU",
                        "llvmpipe (LLVM 17.0.6, 256 bits)",
                    ),
                    (
                        "PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU",
                        "Intel(R) Graphics (RPL-P)",
                    ),
                ]),
            ),
            (
                "no list, but a GPU with its Vulkan driver",
                VulkanDevices::Unlisted { gpu_driver: true },
            ),
        ]
    }
}

#[cfg(test)]
#[path = "vulkan_devices_tests.rs"]
mod vulkan_devices_tests;
