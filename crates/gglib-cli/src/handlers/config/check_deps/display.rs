//! Display utilities for dependency status output.

use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyStatus};

use crate::presentation::style::{BOLD, DANGER, RESET, SUCCESS, WARNING};

/// Print a single dependency row in the status table.
pub fn print_dependency(dep: &Dependency) {
    let status_str = match &dep.status {
        DependencyStatus::Present { version } => {
            if version.is_empty() {
                format!("{}✓ installed{}", SUCCESS, RESET)
            } else {
                format!("{}✓ v{}{}", SUCCESS, version, RESET)
            }
        }
        DependencyStatus::Missing => {
            if dep.required {
                format!("{}✗ missing{}", DANGER, RESET)
            } else {
                format!("{}○ missing{}", WARNING, RESET)
            }
        }
        DependencyStatus::Optional => {
            format!("{}○ optional{}", WARNING, RESET)
        }
    };

    let req_indicator = if dep.required {
        format!("{}*{}", DANGER, RESET)
    } else {
        " ".to_string()
    };

    println!(
        "{}{:<19} {:<25} {}",
        req_indicator, dep.name, status_str, dep.description
    );
}

/// Print GPU detection status and recommendations.
pub fn print_gpu_status(probe: &dyn SystemProbePort) {
    let gpu_info = probe.detect_gpu_info();

    println!("\n{}GPU Detection:{}", BOLD, RESET);
    println!("{}", "-".repeat(40));

    if gpu_info.has_nvidia_gpu {
        println!("  {}✓ NVIDIA GPU detected{}", SUCCESS, RESET);
        if let Some(ref cuda_ver) = gpu_info.cuda_version {
            println!("  {}✓ CUDA available (v{}){}", SUCCESS, cuda_ver, RESET);
        } else {
            println!(
                "  {}! CUDA not found - install CUDA toolkit for GPU acceleration{}",
                WARNING, RESET
            );
        }
    } else if gpu_info.has_metal {
        println!("  {}✓ Metal GPU detected (Apple Silicon){}", SUCCESS, RESET);
        println!("  {}✓ GPU acceleration available{}", SUCCESS, RESET);
    } else if gpu_info.has_vulkan {
        println!("  {}✓ Vulkan GPU detected{}", SUCCESS, RESET);
        if gpu_info.vulkan_headers && gpu_info.vulkan_glslc && gpu_info.vulkan_spirv_headers {
            println!(
                "  {}✓ GPU acceleration available via Vulkan{}",
                SUCCESS, RESET
            );
        } else {
            let mut missing: Vec<&str> = Vec::new();
            if !gpu_info.vulkan_headers {
                missing.push("Vulkan dev headers");
            }
            if !gpu_info.vulkan_glslc {
                missing.push("glslc");
            }
            if !gpu_info.vulkan_spirv_headers {
                missing.push("SPIR-V headers");
            }
            println!(
                "  {}✗ Vulkan loader detected, but build dependencies are missing: {}{}",
                DANGER,
                missing.join(", "),
                RESET
            );
            println!(
                "  {}  Install the missing components above to enable GPU acceleration.{}",
                DANGER, RESET
            );
            println!(
                "  {}  Run `gglib config llama detect` for per-distro install hints.{}",
                DANGER, RESET
            );
        }
    } else {
        println!(
            "  {}✗ No supported GPU detected (Metal/CUDA/Vulkan required){}",
            DANGER, RESET
        );
        println!("  {}  CPU-only inference is not supported{}", DANGER, RESET);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::utils::system::DependencyStatus;

    #[test]
    fn test_print_dependency_present() {
        let dep = Dependency::required("test", "Test dependency").with_status(
            DependencyStatus::Present {
                version: "1.0".to_string(),
            },
        );

        // Just verify it doesn't panic
        print_dependency(&dep);
    }

    #[test]
    fn test_print_dependency_missing_required() {
        let dep =
            Dependency::required("test", "Test dependency").with_status(DependencyStatus::Missing);

        print_dependency(&dep);
    }

    #[test]
    fn test_print_dependency_missing_optional() {
        let dep =
            Dependency::optional("test", "Test dependency").with_status(DependencyStatus::Missing);

        print_dependency(&dep);
    }
}
