//! Tests for [`super::parse_vulkaninfo`], [`super::parse_vram_sysfs`] and the
//! budget decision.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.
//!
//! The integrated fixture is **real output**, captured from `vulkaninfo` on an
//! AMD Radeon 840M (RADV KRACKAN1) on 2026-08-28 — the machine that
//! prompted the three-way [`super::DeviceKind`]. Its numbers are what the
//! parser must survive, not what a hand-written sample makes convenient.

use super::*;

/// Real `vulkaninfo` output, elided between the device block and the heaps.
const KRACKAN_IGPU: &str = "\
\tdeviceType        = PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU
\tdeviceName        = AMD Radeon 840M Graphics (RADV KRACKAN1)
VkPhysicalDeviceMemoryProperties:
=================================
memoryHeaps: count = 2
\tmemoryHeaps[0]:
\t\tsize   = 6270984192 (0x175c7a000) (5.84 GiB)
\t\tbudget = 5911248896 (0x160568000) (5.51 GiB)
\t\tusage  = 0 (0x00000000) (0.00 B)
\t\tflags:
\t\t\tNone
\tmemoryHeaps[1]:
\t\tsize   = 12541968384 (0x2eb8f4000) (11.68 GiB)
\t\tbudget = 11822497792 (0x2c0ad0000) (11.01 GiB)
\t\tusage  = 0 (0x00000000) (0.00 B)
\t\tflags: count = 1
\t\t\tMEMORY_HEAP_DEVICE_LOCAL_BIT
memoryTypes: count = 11
\tmemoryTypes[0]:
\t\theapIndex     = 1
";

/// A discrete card, same shape, with the device-local heap listed first.
const DISCRETE_GPU: &str = "\
\tdeviceType        = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU
\tdeviceName        = AMD Radeon RX 7900 XTX
memoryHeaps: count = 2
\tmemoryHeaps[0]:
\t\tsize   = 25753026560 (0x5fe000000) (23.98 GiB)
\t\tflags: count = 1
\t\t\tMEMORY_HEAP_DEVICE_LOCAL_BIT
\tmemoryHeaps[1]:
\t\tsize   = 8589934592 (0x200000000) (8.00 GiB)
\t\tflags:
\t\t\tNone
memoryTypes: count = 4
";

#[test]
fn the_krackan_igpu_parses_as_integrated_with_its_largest_local_heap() {
    let device = parse_vulkaninfo(KRACKAN_IGPU).expect("deviceType is present");
    assert_eq!(device.kind, DeviceKind::Integrated);
    // The device-local heap, not the 5.84 GiB one that carries no flags.
    assert_eq!(device.device_local_bytes, Some(12_541_968_384));
    // Every heap, flagged or not. amdgpu's GTT + VRAM sum to exactly this.
    assert_eq!(device.total_heap_bytes, Some(18_812_952_576));
}

/// The share must not promise more than the device says it has.
///
/// Apple's 0.75 is backed by a platform limit. amdgpu's GTT ceiling is a
/// kernel default — ~50% of RAM here — and it is reported, so an uncapped
/// share over-promises: 0.75 x 27.04 GiB is 20.28 GiB against 17.52 GiB of
/// total heap. A model landing between the two would pass the fit and then
/// fail to allocate.
#[test]
fn the_unified_share_is_capped_by_what_the_device_reports() {
    let device = parse_vulkaninfo(KRACKAN_IGPU).expect("deviceType is present");
    let total_ram = 29_035_974_656;

    assert!(
        unified_share(total_ram) > 18_812_952_576,
        "the cap must bind"
    );
    assert_eq!(
        budget_for(&device, total_ram),
        Some(18_812_952_576),
        "capped at the heap total, not the raw 0.75 share"
    );
}

/// A device that reported no heaps at all still gets the uncapped share —
/// there is nothing to cap against, and refusing would be worse than Apple's
/// own answer for the same architecture.
#[test]
fn an_integrated_gpu_with_no_heap_total_falls_back_to_the_plain_share() {
    let device = VulkanDevice {
        kind: DeviceKind::Integrated,
        device_local_bytes: None,
        total_heap_bytes: None,
    };

    assert_eq!(
        budget_for(&device, 29_035_974_656),
        Some(unified_share(29_035_974_656))
    );
}

/// A second device block ends the first, even with no `memoryTypes:` line.
///
/// `in_heaps` never resets, so without the guard a later GPU's 64 GiB heap
/// would be folded into device 0's budget. Real `vulkaninfo` always prints the
/// terminator; nothing in the format guarantees it.
#[test]
fn a_second_device_block_does_not_leak_into_the_first() {
    let two = "\tdeviceType = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU\n\
               memoryHeaps: count = 1\n\
               \tmemoryHeaps[0]:\n\
               \t\tsize   = 1073741824 (0x40000000) (1.00 GiB)\n\
               \t\tflags: count = 1\n\
               \t\t\tMEMORY_HEAP_DEVICE_LOCAL_BIT\n\
               \tdeviceType = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU\n\
               \tmemoryHeaps[0]:\n\
               \t\tsize   = 68719476736 (0x1000000000) (64.00 GiB)\n\
               \t\tflags: count = 1\n\
               \t\t\tMEMORY_HEAP_DEVICE_LOCAL_BIT\n";

    let device = parse_vulkaninfo(two).expect("deviceType is present");

    assert_eq!(
        device.device_local_bytes,
        Some(1_073_741_824),
        "the second device's 64 GiB heap is not this device's VRAM"
    );
}

/// The whole point of [`super::DeviceKind::Integrated`].
///
/// 11.68 GiB is carved from host RAM, not the card's own memory — on a machine whose VRAM
/// carve-out is 4 GiB. Reporting it would size a KV cache against system
/// memory, which is what the refusal in `total_device_memory_bytes` exists to
/// prevent. The unified rule is applied instead, capped by the heap total.
#[test]
fn an_integrated_gpu_is_not_sized_by_its_device_local_heap() {
    let device = parse_vulkaninfo(KRACKAN_IGPU).expect("deviceType is present");

    let budget = budget_for(&device, 29_035_974_656).expect("an integrated GPU gets a budget");

    assert_ne!(
        budget,
        device.device_local_bytes.expect("the fixture has one"),
        "the device-local heap is host RAM on an iGPU and must not be the budget"
    );
    assert!(
        budget <= device.total_heap_bytes.expect("the fixture has one"),
        "and it never promises more than the device reports"
    );
}

#[test]
fn a_discrete_gpu_is_sized_by_its_device_local_heap() {
    let device = parse_vulkaninfo(DISCRETE_GPU).expect("deviceType is present");
    assert_eq!(device.kind, DeviceKind::Discrete);
    assert_eq!(
        budget_for(&device, 29_030_000_000),
        Some(25_753_026_560),
        "a discrete card's device-local heap is its own VRAM"
    );
}

/// A software rasteriser reports `DEVICE_LOCAL` heaps like anything else, and
/// they are plain host RAM. This is why [`super::DeviceKind`] is not a bool.
#[test]
fn a_software_rasteriser_gets_no_budget_despite_its_heaps() {
    let lavapipe = DISCRETE_GPU.replace(
        "PHYSICAL_DEVICE_TYPE_DISCRETE_GPU",
        "PHYSICAL_DEVICE_TYPE_CPU",
    );

    let device = parse_vulkaninfo(&lavapipe).expect("deviceType is present");

    assert_eq!(device.kind, DeviceKind::Other);
    assert_eq!(
        device.device_local_bytes,
        Some(25_753_026_560),
        "the heap is still read — it is the verdict that refuses, not the parse"
    );
    assert_eq!(budget_for(&device, 29_030_000_000), None);
}

#[test]
fn the_first_device_wins_on_a_multi_gpu_host() {
    let both = format!("{KRACKAN_IGPU}{DISCRETE_GPU}");

    let device = parse_vulkaninfo(&both).expect("deviceType is present");

    assert_eq!(
        device.kind,
        DeviceKind::Integrated,
        "the first block decides, matching nvidia-smi's rule"
    );
}

#[test]
fn output_with_no_device_type_is_a_refusal() {
    assert_eq!(parse_vulkaninfo(""), None);
    assert_eq!(parse_vulkaninfo("Vulkan Instance Version: 1.4.357\n"), None);
}

#[test]
fn a_device_with_no_local_heap_reports_none_rather_than_zero() {
    let heapless = "\tdeviceType = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU\n";

    let device = parse_vulkaninfo(heapless).expect("deviceType is present");

    assert_eq!(device.device_local_bytes, None);
    assert_eq!(budget_for(&device, 29_030_000_000), None);
}

#[test]
fn sysfs_vram_is_a_plain_byte_count() {
    assert_eq!(parse_vram_sysfs("4294967296\n"), Some(4_294_967_296));
    assert_eq!(parse_vram_sysfs("  4294967296  "), Some(4_294_967_296));
}

/// The kernel reports `0` for a device whose VRAM could not be sized.
#[test]
fn a_zero_sysfs_reading_is_a_refusal_not_a_budget() {
    assert_eq!(parse_vram_sysfs("0\n"), None);
    assert_eq!(parse_vram_sysfs(""), None);
    assert_eq!(parse_vram_sysfs("not a number"), None);
}

#[test]
fn the_unified_share_is_the_one_apple_silicon_already_gets() {
    assert_eq!(unified_share(32 * 1024 * 1024 * 1024), 25_769_803_776);
    assert_eq!(unified_share(0), 0);
}
