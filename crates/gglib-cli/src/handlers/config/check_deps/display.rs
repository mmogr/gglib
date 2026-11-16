//! Display utilities for dependency status output.

use std::fmt::Write as _;

use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyReport, DependencyStatus};

use crate::presentation::style::{BOLD, DANGER, DIM, RESET, SUCCESS, WARNING};

const RULE_WIDTH: usize = 85;

/// The report as `check-deps` prints it: each group's heading and what it
/// says about this gglib, its rows, then the conclusion. Every word comes from
/// the report, which is also what the app's diagnostics show.
pub(super) fn render_report(report: &DependencyReport) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{BOLD}{:<20} {:<15} {:<50}{RESET}",
        "DEPENDENCY", "STATUS", "NOTES"
    );
    let _ = writeln!(out, "{}", "=".repeat(RULE_WIDTH));

    for group in &report.groups {
        let _ = writeln!(out, "\n{BOLD}{}{RESET}", group.title);
        let _ = writeln!(out, "{DIM}{}{RESET}", group.explanation);
        for dep in &group.dependencies {
            let _ = writeln!(out, "{}", dependency_row(dep));
        }
    }

    let _ = writeln!(out, "\n{}", "=".repeat(RULE_WIDTH));
    let verdict = report.verdict();
    if report.missing().is_empty() {
        let _ = writeln!(out, "{SUCCESS}✓ {verdict}{RESET}");
    } else {
        let _ = writeln!(out, "{DANGER}✗ {verdict}{RESET}");
    }
    out
}

/// A single dependency row of the status table. A `*` marks what this gglib
/// needs; the state is [`Dependency::status_word`], the app's word too.
fn dependency_row(dep: &Dependency) -> String {
    let word = dep.status_word();
    let status_str = match &dep.status {
        DependencyStatus::Present { .. } => format!("{SUCCESS}✓ {word}{RESET}"),
        DependencyStatus::Missing if dep.required => format!("{DANGER}✗ {word}{RESET}"),
        DependencyStatus::Missing | DependencyStatus::Optional => {
            format!("{WARNING}○ {word}{RESET}")
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

/// Print GPU detection status and recommendations. `builds_engine` is
/// whether this gglib builds its engines, which is when Vulkan's build
/// packages decide whether a GPU is used.
pub(super) fn print_gpu_status(probe: &dyn SystemProbePort, builds_engine: bool) {
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
        let build_ready =
            gpu_info.vulkan_headers && gpu_info.vulkan_glslc && gpu_info.vulkan_spirv_headers;
        if build_ready || !builds_engine {
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
#[path = "display_tests.rs"]
mod tests;
