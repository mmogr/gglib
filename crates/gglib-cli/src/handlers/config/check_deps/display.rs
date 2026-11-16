//! Display utilities for dependency status output.

use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyStatus};

use crate::presentation::style::{BOLD, DANGER, RESET, SUCCESS, WARNING};

/// Print a single dependency row in the status table.
pub(super) fn print_dependency(dep: &Dependency) {
    println!("{}", dependency_row(dep));
}

/// A single dependency row of the status table.
fn dependency_row(dep: &Dependency) -> String {
    let status_str = match &dep.status {
        DependencyStatus::Present { version } => {
            if version.is_empty() {
                format!("{SUCCESS}✓ installed{RESET}")
            } else {
                format!("{SUCCESS}✓ v{version}{RESET}")
            }
        }
        DependencyStatus::Missing => {
            if dep.required {
                format!("{DANGER}✗ missing{RESET}")
            } else {
                format!("{WARNING}○ missing{RESET}")
            }
        }
        DependencyStatus::Optional => {
            format!("{WARNING}○ optional{RESET}")
        }
    };

    let req_indicator = if dep.required {
        format!("{DANGER}*{RESET}")
    } else {
        " ".to_string()
    };

    format!(
        "{}{:<19} {:<25} {}",
        req_indicator, dep.name, status_str, dep.description
    )
}

/// Print GPU detection status and recommendations.
pub(super) fn print_gpu_status(probe: &dyn SystemProbePort) {
    let gpu_info = probe.detect_gpu_info();

    println!("\n{BOLD}GPU Detection:{RESET}");
    println!("{}", "-".repeat(40));

    if gpu_info.has_nvidia_gpu {
        println!("  {SUCCESS}✓ NVIDIA GPU detected{RESET}");
        if let Some(ref cuda_ver) = gpu_info.cuda_version {
            println!("  {SUCCESS}✓ CUDA available (v{cuda_ver}){RESET}");
        } else {
            println!(
                "  {WARNING}! CUDA not found - install CUDA toolkit for GPU acceleration{RESET}"
            );
        }
    } else if gpu_info.has_metal {
        println!("  {SUCCESS}✓ Metal GPU detected (Apple Silicon){RESET}");
        println!("  {SUCCESS}✓ GPU acceleration available{RESET}");
    } else if gpu_info.has_vulkan {
        println!("  {SUCCESS}✓ Vulkan GPU detected{RESET}");
        if gpu_info.vulkan_headers && gpu_info.vulkan_glslc && gpu_info.vulkan_spirv_headers {
            println!("  {SUCCESS}✓ GPU acceleration available via Vulkan{RESET}");
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
                "  {DANGER}  Install the missing components above to enable GPU acceleration.{RESET}"
            );
            println!(
                "  {DANGER}  Run `gglib config llama detect` for per-distro install hints.{RESET}"
            );
        }
    } else {
        println!("  {WARNING}○ No supported GPU detected (Metal, CUDA or Vulkan){RESET}");
        println!("  {WARNING}  llama.cpp runs on the CPU here, and replies will be slow{RESET}");
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

    /// C11: a probe that found something without a version, such as Metal,
    /// printed as "✓ vavailable".
    #[test]
    fn a_word_where_a_version_would_be_takes_no_v() {
        let available =
            Dependency::optional("Metal", "Apple GPU").with_status(DependencyStatus::Present {
                version: "available".to_string(),
            });
        let row = dependency_row(&available);
        assert!(row.contains("✓ available"), "{row}");

        let versioned =
            Dependency::required("cmake", "Build").with_status(DependencyStatus::Present {
                version: "3.28.1".to_string(),
            });
        assert!(dependency_row(&versioned).contains("✓ v3.28.1"));
    }

    #[test]
    fn test_print_dependency_missing_optional() {
        let dep =
            Dependency::optional("test", "Test dependency").with_status(DependencyStatus::Missing);

        print_dependency(&dep);
    }
}
