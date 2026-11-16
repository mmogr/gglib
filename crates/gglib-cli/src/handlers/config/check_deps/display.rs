//! Display utilities for dependency status output.

use std::fmt::Write as _;

use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyReport, DependencyStatus};

use crate::presentation::style::{BOLD, DANGER, DIM, RESET, SUCCESS, WARNING};
use crate::presentation::table_layout::{Column, Shorten, Table, rule, wrap};
use crate::presentation::terminal::stdout_width;

/// The report as `check-deps` prints it: each group's heading and what it
/// says about this gglib, its rows, then the conclusion. Every word comes from
/// the report, which is also what the app's diagnostics show. The rows of
/// every group are one table, so their columns line up across groups; it
/// fits `width`, the terminal's, or is whole when there is none. A note
/// wraps before it is dropped; a name and a state are never shortened.
pub(super) fn render_report(report: &DependencyReport, width: Option<usize>) -> String {
    let mut table = Table::new(vec![
        Column::new("DEPENDENCY").pad(20),
        Column::new("STATUS").pad(15),
        Column::new("NOTES")
            .shorten(Shorten::Wrap { min: 20 })
            .droppable(1),
    ])
    .ruler('=');
    for group in &report.groups {
        for dep in &group.dependencies {
            table.row(dependency_cells(dep));
        }
    }
    let laid = table.lay_out(width);
    let closing = laid
        .ruler
        .clone()
        .unwrap_or_else(|| rule('=', width.unwrap_or_default(), width));

    let mut out = String::new();
    if let Some(header) = &laid.header {
        let _ = writeln!(out, "{BOLD}{header}{RESET}");
    }
    if let Some(ruler) = &laid.ruler {
        let _ = writeln!(out, "{ruler}");
    }
    let mut rows = laid.rows.iter();
    for group in &report.groups {
        let _ = writeln!(out, "\n{BOLD}{}{RESET}", group.title);
        for line in prose(&group.explanation, width) {
            let _ = writeln!(out, "{DIM}{line}{RESET}");
        }
        for row in rows.by_ref().take(group.dependencies.len()) {
            if laid.is_blocks() {
                out.push('\n');
            }
            for line in row {
                let _ = writeln!(out, "{line}");
            }
        }
    }

    let _ = writeln!(out, "\n{closing}");
    let (colour, mark) = if report.missing().is_empty() {
        (SUCCESS, '✓')
    } else {
        (DANGER, '✗')
    };
    for line in prose(&format!("{mark} {}", report.verdict()), width) {
        let _ = writeln!(out, "{colour}{line}{RESET}");
    }
    out
}

/// A sentence of the report, wrapped to `width` when there is one.
fn prose(text: &str, width: Option<usize>) -> Vec<String> {
    width.map_or_else(|| vec![text.to_owned()], |width| wrap(text, width))
}

/// A single dependency row of the status table, one cell per column. A `*`
/// marks what this gglib needs; the state is [`Dependency::status_word`],
/// the app's word too.
fn dependency_cells(dep: &Dependency) -> Vec<String> {
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

    vec![
        format!("{req_indicator}{}", dep.name),
        status_str,
        dep.description.clone(),
    ]
}

/// Print GPU detection status and recommendations. `builds_engine` is
/// whether this gglib builds its engines, which is when Vulkan's build
/// packages decide whether a GPU is used.
pub(super) fn print_gpu_status(probe: &dyn SystemProbePort, builds_engine: bool) {
    let gpu_info = probe.detect_gpu_info();

    println!("\n{BOLD}GPU Detection:{RESET}");
    println!("{}", rule('-', 40, stdout_width()));

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
