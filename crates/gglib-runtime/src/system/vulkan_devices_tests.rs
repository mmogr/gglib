//! The Vulkan devices a machine has, and which of them a Vulkan build runs on.

use super::{gpu_device_under, vulkan_driver_under};

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
