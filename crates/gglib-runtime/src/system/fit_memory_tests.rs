//! The budget decision for each shape of machine, as values.

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

fn memory(total_ram_bytes: u64, gpu_memory_bytes: Option<u64>) -> SystemMemoryInfo {
    SystemMemoryInfo {
        total_ram_bytes,
        gpu_memory_bytes,
        is_unified_memory: false,
        has_nvidia_gpu: false,
    }
}

/// A machine with a GPU whose VRAM gglib cannot read must get no budget at
/// all, never host RAM. Sizing a KV cache against 64 GiB of system memory on
/// an 8 GiB card is the "working but unusably slow" outcome the
/// recommendation module exists to prevent, and it would look perfectly
/// stable while measuring the wrong device.
#[test]
fn a_gpu_whose_memory_cannot_be_read_gets_no_budget_rather_than_host_ram() {
    let vulkan_box = memory(64 * GIB, None);
    assert_eq!(fit_memory_of(true, &vulkan_box), FitMemory::Unreadable);
    assert_eq!(fit_memory_of(true, &vulkan_box).budget_bytes(), None);
}

/// A readable device reports what it reported, GPU or not: an amdgpu card
/// with no Vulkan loader is read from sysfs and counts as no Vulkan GPU.
#[test]
fn a_readable_device_reports_its_own_memory() {
    let card = SystemMemoryInfo {
        has_nvidia_gpu: true,
        ..memory(64 * GIB, Some(24 * GIB))
    };
    for present in [true, false] {
        assert_eq!(
            fit_memory_of(present, &card),
            FitMemory::Device { bytes: 24 * GIB }
        );
    }
}

/// Apple Silicon's unified memory is a device reading, as it always was.
#[test]
fn unified_memory_is_the_devices_reading() {
    let mac = SystemMemoryInfo {
        is_unified_memory: true,
        ..memory(32 * GIB, Some(24 * GIB))
    };
    assert_eq!(
        fit_memory_of(true, &mac),
        FitMemory::Device { bytes: 24 * GIB }
    );
}

/// With no GPU at all the model runs from RAM, and the budget is the share
/// of RAM a unified-memory GPU is given (three quarters), not a refusal.
#[test]
fn a_machine_with_no_gpu_is_sized_against_its_ram() {
    let cpu_box = memory(64 * GIB, None);
    let got = fit_memory_of(false, &cpu_box);
    assert_eq!(
        got,
        FitMemory::SystemRam {
            total_bytes: 64 * GIB,
            budget_bytes: 48 * GIB,
        }
    );
    assert_eq!(got.budget_bytes(), Some(48 * GIB));
    assert_eq!(got.system_ram_bytes(), Some(64 * GIB));
    assert_eq!(got.device_bytes(), None, "RAM is no GPU's memory");
}

/// A machine whose RAM could not be read either has nothing to size
/// against.
#[test]
fn a_machine_with_no_gpu_and_no_ram_reading_gets_no_budget() {
    assert_eq!(
        fit_memory_of(false, &memory(0, None)),
        FitMemory::Unreadable
    );
}

/// Each of Metal, NVIDIA and a Vulkan GPU is a GPU; none of them is none.
#[test]
fn any_one_gpu_is_a_gpu() {
    let none = GpuInfo::default();
    assert!(!a_gpu_is_present(&none));
    for gpu in [
        GpuInfo {
            has_metal: true,
            ..none.clone()
        },
        GpuInfo {
            has_nvidia_gpu: true,
            ..none.clone()
        },
        GpuInfo {
            has_vulkan: true,
            ..none
        },
    ] {
        assert!(a_gpu_is_present(&gpu), "{gpu:?}");
    }
}
