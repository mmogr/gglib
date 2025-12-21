#![doc = include_str!("README.md")]

//! Check system dependencies handler.
//!
//! This module handles checking for required system dependencies
//! and displaying them in a formatted, user-friendly way.

mod display;
mod instructions;
mod platform;

use anyhow::{Context, Result};
use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyStatus};
use gglib_download::cli_exec::ensure_fast_helper_ready;

use display::{print_dependency, print_gpu_status};
use instructions::print_installation_instructions;

// ANSI color codes for better UX
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const BLUE: &str = "\x1b[34m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Execute the check-deps command.
///
/// Checks for all required and optional dependencies,
/// displays them in a formatted table, and returns an appropriate
/// exit code based on whether all required dependencies are present.
///
/// # Arguments
///
/// * `probe` - System probe implementation for dependency detection
///
/// # Returns
///
/// Returns `Ok(())` if all required dependencies are present.
/// Returns an error if any required dependencies are missing.
pub async fn execute(probe: &dyn SystemProbePort) -> Result<()> {
    println!("{}{}Checking system dependencies...{}\n", BOLD, BLUE, RESET);

    let dependencies = probe.check_all_dependencies();

    println!(
        "{}{:<20} {:<15} {:<50}{}",
        BOLD, "DEPENDENCY", "STATUS", "NOTES", RESET
    );
    println!("{}", "=".repeat(85));

    for dep in &dependencies {
        print_dependency(dep);
    }

    println!();

    let missing_required: Vec<&Dependency> = dependencies
        .iter()
        .filter(|d| d.required && matches!(d.status, DependencyStatus::Missing))
        .collect();

    let present_required = dependencies
        .iter()
        .filter(|d| d.required && matches!(d.status, DependencyStatus::Present { .. }))
        .count();
    let total_required = dependencies.iter().filter(|d| d.required).count();

    println!("{}", "=".repeat(85));
    if missing_required.is_empty() {
        println!(
            "{}✓ All required dependencies are installed!{} ({}/{})",
            GREEN, RESET, present_required, total_required
        );

        println!(
            "{}Ensuring fast download helper is installed...{}",
            BOLD, RESET
        );
        ensure_fast_helper_ready()
            .await
            .context("Failed to set up the Python fast download helper")?;
        println!("{}✓ Fast download helper ready{}", GREEN, RESET);

        print_gpu_status(probe);

        println!("\n{}You can now run: {}make setup{}", BOLD, BLUE, RESET);
        Ok(())
    } else {
        println!(
            "{}✗ {} required dependencies are missing.{} ({}/{})",
            RED,
            missing_required.len(),
            RESET,
            present_required,
            total_required
        );
        println!();
        print_installation_instructions(&missing_required);
        anyhow::bail!("Missing required dependencies")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_handler_exists() {
        // Placeholder test to ensure module compiles
    }
}
