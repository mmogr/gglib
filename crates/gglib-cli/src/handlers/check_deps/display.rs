//! Display utilities for dependency status output.

use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyStatus};

// ANSI color codes
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Print a single dependency row in the status table.
pub fn print_dependency(dep: &Dependency) {
    let status_str = match &dep.status {
        DependencyStatus::Present { version } => {
            if version.is_empty() {
                format!("{}✓ installed{}", GREEN, RESET)
            } else {
                format!("{}✓ v{}{}", GREEN, version, RESET)
            }
        }
        DependencyStatus::Missing => {
            if dep.required {
                format!("{}✗ missing{}", RED, RESET)
            } else {
                format!("{}○ missing{}", YELLOW, RESET)
            }
        }
        DependencyStatus::Optional => {
            format!("{}○ optional{}", YELLOW, RESET)
        }
    };

    let req_indicator = if dep.required {
        format!("{}*{}", RED, RESET)
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
        println!("  {}✓ NVIDIA GPU detected{}", GREEN, RESET);
        if let Some(ref cuda_ver) = gpu_info.cuda_version {
            println!("  {}✓ CUDA available (v{}){}", GREEN, cuda_ver, RESET);
        } else {
            println!(
                "  {}! CUDA not found - install CUDA toolkit for GPU acceleration{}",
                YELLOW, RESET
            );
        }
    } else if gpu_info.has_metal {
        println!("  {}✓ Metal GPU detected (Apple Silicon){}", GREEN, RESET);
        println!("  {}✓ GPU acceleration available{}", GREEN, RESET);
    } else {
        println!(
            "  {}○ No dedicated GPU detected - CPU inference will be used{}",
            YELLOW, RESET
        );
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
