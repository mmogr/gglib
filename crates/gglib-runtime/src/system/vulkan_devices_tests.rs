//! The Vulkan devices a machine has, and which of them a Vulkan build runs on.

use super::readers::{
    adapter_keys, gpu_device_under, names_a_driver, parse_summary, vulkan_driver_under,
};
use super::{VulkanDevice, VulkanDevices, fixtures, listed_or, vulkan_gpu_present};

#[test]
fn a_vulkan_driver_is_an_icd_manifest() {
    let share = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    assert!(
        !vulkan_driver_under(&[share.path(), etc.path()]),
        "no manifests"
    );
    std::fs::write(share.path().join("README"), "").unwrap();
    assert!(
        !vulkan_driver_under(&[share.path(), etc.path()]),
        "not a manifest"
    );
    std::fs::write(etc.path().join("radeon_icd.x86_64.json"), "{}").unwrap();
    assert!(vulkan_driver_under(&[share.path(), etc.path()]));
    assert!(
        !vulkan_driver_under(&[&share.path().join("gone")]),
        "a missing directory"
    );
}

#[test]
fn a_gpu_is_a_render_node_or_nvidias_or_wsl2s_device_node() {
    let dev = tempfile::tempdir().unwrap();
    assert!(!gpu_device_under(dev.path()), "an empty /dev");

    std::fs::create_dir_all(dev.path().join("dri")).unwrap();
    std::fs::write(dev.path().join("dri/card0"), "").unwrap();
    assert!(
        !gpu_device_under(dev.path()),
        "a display card alone renders nothing"
    );

    std::fs::write(dev.path().join("dri/renderD128"), "").unwrap();
    assert!(gpu_device_under(dev.path()));

    let nvidia = tempfile::tempdir().unwrap();
    std::fs::write(nvidia.path().join("nvidia0"), "").unwrap();
    assert!(gpu_device_under(nvidia.path()));

    let wsl = tempfile::tempdir().unwrap();
    std::fs::write(wsl.path().join("dxg"), "").unwrap();
    assert!(gpu_device_under(wsl.path()), "WSL2's GPU");
}

/// What `vulkaninfo --summary` printed under `Devices:` on a laptop with an
/// AMD Radeon 840M (vulkaninfo 1.4.354, Mesa 26.2.6), 2026-11-16.
const RADEON_840M_SUMMARY: &str = "\
Devices:
========
GPU0:
\tapiVersion         = 1.4.354
\tdriverVersion      = 26.2.6
\tvendorID           = 0x1002
\tdeviceID           = 0x1114
\tdeviceType         = PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU
\tdeviceName         = AMD Radeon 840M Graphics (RADV KRACKAN1)
\tdriverID           = DRIVER_ID_MESA_RADV
\tdriverName         = radv
\tdriverInfo         = Mesa 26.2.6-arch3.1
\tconformanceVersion = 1.4.5.3
\tdeviceUUID         = 00000000-c300-0000-0000-000000000000
\tdriverUUID         = 414d442d-4d45-5341-2d44-525600000000
";

fn device(device_type: &str, name: &str) -> VulkanDevice {
    VulkanDevice {
        device_type: device_type.to_owned(),
        name: name.to_owned(),
    }
}

#[test]
fn a_summary_lists_each_device_by_its_type_and_name() {
    assert_eq!(
        parse_summary(RADEON_840M_SUMMARY),
        [device(
            "PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU",
            "AMD Radeon 840M Graphics (RADV KRACKAN1)"
        )]
    );

    // The same layout with a second device, as Mesa adds llvmpipe, and the
    // instance lines above the devices, which are not devices.
    let two = format!(
        "Vulkan Instance Version: 1.4.354\n\nInstance Extensions: count = 24\n\
         VK_KHR_surface : extension revision 25\n{RADEON_840M_SUMMARY}\
         GPU1:\n\tdeviceType         = PHYSICAL_DEVICE_TYPE_CPU\n\
         \tdeviceName         = llvmpipe (LLVM 21.1.6, 256 bits)\n"
    );
    let listed = parse_summary(&two);
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert_eq!(
        listed[1],
        device(
            "PHYSICAL_DEVICE_TYPE_CPU",
            "llvmpipe (LLVM 21.1.6, 256 bits)"
        )
    );

    assert!(parse_summary("").is_empty());
}

#[test]
fn a_gpu_is_a_gpus_type_and_not_a_software_rasterisers_name() {
    for gpu in [
        device(
            "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU",
            "NVIDIA GeForce RTX 4090",
        ),
        device(
            "PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU",
            "Intel(R) Arc(TM) Graphics",
        ),
        device(
            "PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU",
            "Virtio-GPU Venus (AMD Radeon 840M)",
        ),
        device("VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU", "AMD Radeon RX 7600"),
    ] {
        assert!(gpu.is_a_gpu(), "{gpu:?}");
    }
    for not_a_gpu in [
        device(
            "PHYSICAL_DEVICE_TYPE_CPU",
            "llvmpipe (LLVM 17.0.6, 256 bits)",
        ),
        device("PHYSICAL_DEVICE_TYPE_OTHER", "some device"),
        device("PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU", "LLVMpipe"),
        device(
            "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU",
            "SwiftShader Device (Subzero) (0x0000C0DE)",
        ),
        device(
            "PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU",
            "Microsoft Direct3D12 (Microsoft Basic Render Driver)",
        ),
        device("", ""),
    ] {
        assert!(!not_a_gpu.is_a_gpu(), "{not_a_gpu:?}");
    }
}

/// macOS's one build is Metal, whatever a Vulkan list there would say; and
/// with no loader, nothing runs a Vulkan build.
#[test]
fn macos_and_a_missing_loader_never_take_vulkan() {
    for (what, devices) in fixtures::with_a_gpu() {
        assert!(!vulkan_gpu_present("macos", true, &devices), "{what}");
        assert!(!vulkan_gpu_present("linux", false, &devices), "{what}");
        assert!(!vulkan_gpu_present("windows", false, &devices), "{what}");
    }
}

/// A list with a device in it is the answer, and the operating system is
/// not asked; an empty one is no answer, and it is.
#[test]
fn an_empty_list_asks_the_operating_system() {
    let gpu = device("PHYSICAL_DEVICE_TYPE_DISCRETE_GPU", "AMD Radeon RX 7600");
    assert_eq!(
        listed_or(vec![gpu.clone()], || panic!("not asked")),
        VulkanDevices::Listed(vec![gpu])
    );
    assert_eq!(
        listed_or(Vec::new(), || true),
        VulkanDevices::Unlisted { gpu_driver: true }
    );
    assert_eq!(
        listed_or(Vec::new(), || false),
        VulkanDevices::Unlisted { gpu_driver: false }
    );
}

/// `reg query`'s listing of the display adapters' class on the Windows 11
/// test VM, which has Microsoft's Basic Display Adapter alone, 2026-11-16.
const VM_ADAPTER_LISTING: &str = "\r
HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\r
    Class    REG_SZ    Display\r
    ClassDesc    REG_SZ    @c_display.inf,%ClassDesc%;Display adapters\r
    IconPath    REG_MULTI_SZ    %SystemRoot%\\system32\\setupapi.dll,-1\r
    LowerLogoVersion    REG_SZ    6.0\r
\r
HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0000\r
HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\Configuration\r
HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\Properties\r
";

#[test]
fn the_adapters_are_the_numbered_subkeys_of_the_display_class() {
    assert_eq!(
        adapter_keys(VM_ADAPTER_LISTING).collect::<Vec<_>>(),
        [
            "HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0000"
        ]
    );
    assert_eq!(adapter_keys("").count(), 0);
}

#[test]
fn a_khronos_driver_is_a_manifest_named_in_the_list() {
    assert!(names_a_driver(
        "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Khronos\\Vulkan\\Drivers\r\n    \
         C:\\Windows\\System32\\amd-vulkan64.JSON    REG_DWORD    0x0\r\n"
    ));
    assert!(!names_a_driver(
        "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Khronos\\Vulkan\\Drivers\r\n"
    ));
}
