//! Which Vulkan devices this machine has, and whether one of them is a GPU a
//! Vulkan build of an engine can run on.
//!
//! The one owner of that question: every report of a Vulkan GPU and every
//! choice between an engine's Vulkan and CPU builds reads
//! [`vulkan_gpu_present`], and only [`vulkan_devices`] asks the machine.

/// One device as the Vulkan driver lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VulkanDevice {
    /// Its `deviceType`, as printed: `PHYSICAL_DEVICE_TYPE_DISCRETE_GPU`.
    pub(crate) device_type: String,
    /// Its `deviceName`: `AMD Radeon RX 7900 XTX (RADV NAVI31)`.
    pub(crate) name: String,
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
/// and the `devices` found.
///
/// On Linux a device must be there too; elsewhere the loader comes with a
/// GPU's driver, so it counts.
pub(crate) fn vulkan_gpu_present(os: &str, has_loader: bool, devices: &VulkanDevices) -> bool {
    let any_device = match devices {
        VulkanDevices::Listed(listed) => !listed.is_empty(),
        VulkanDevices::Unlisted { gpu_driver } => *gpu_driver,
    };
    has_loader && (os != "linux" || any_device)
}

/// This machine's Vulkan devices. On Linux, a GPU device: a DRM render node
/// (`/dev/dri/renderD*`, which an AMD, Intel or NVIDIA driver makes),
/// NVIDIA's own device node, or WSL2's GPU (`/dev/dxg`); and a Vulkan driver
/// for it, an ICD manifest, since the loader library is often installed by
/// other packages with no driver at all.
pub(crate) fn vulkan_devices() -> VulkanDevices {
    #[cfg(target_os = "linux")]
    let gpu_driver = || {
        gpu_device_under(std::path::Path::new("/dev"))
            && vulkan_driver_under(&[
                std::path::Path::new("/usr/share/vulkan/icd.d"),
                std::path::Path::new("/etc/vulkan/icd.d"),
            ])
    };
    #[cfg(not(target_os = "linux"))]
    let gpu_driver = || false;
    // No list of devices is read yet.
    listed_or(Vec::new(), gpu_driver)
}

/// The devices `listed`, when the list holds one; else whether the
/// operating system shows a GPU with its Vulkan driver, asked of
/// `gpu_driver`.
fn listed_or(listed: Vec<VulkanDevice>, gpu_driver: impl FnOnce() -> bool) -> VulkanDevices {
    if listed.is_empty() {
        VulkanDevices::Unlisted {
            gpu_driver: gpu_driver(),
        }
    } else {
        VulkanDevices::Listed(listed)
    }
}

/// Whether one of the ICD directories `dirs` holds a Vulkan driver's
/// manifest, a `.json` file.
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "Linux's driver manifests")
)]
fn vulkan_driver_under(dirs: &[&std::path::Path]) -> bool {
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
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "Linux's device nodes")
)]
fn gpu_device_under(dev: &std::path::Path) -> bool {
    let render_node = std::fs::read_dir(dev.join("dri")).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
    });
    render_node || dev.join("nvidia0").exists() || dev.join("dxg").exists()
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
