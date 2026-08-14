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
use gglib_download::cli_exec::{ensure_fast_helper_ready, fast_helper_provisioned};

use display::{print_dependency, print_gpu_status};
use instructions::print_installation_instructions;

use crate::presentation::style::{BOLD, DANGER, INFO, RESET, SUCCESS};

/// Execute the check-deps command.
///
/// Checks for all required and optional dependencies,
/// displays them in a formatted table, and returns an appropriate
/// exit code based on whether all required dependencies are present.
///
/// # Arguments
///
/// * `probe` - System probe implementation for dependency detection
/// * `setup_fast_downloads` - Provision the optional `hf_xet` accelerator.
///   Reporting is side-effect free; this is the only thing that installs
///   anything, and it is off unless the user asks for it. Superseded by
///   `gglib config fast-downloads enable`, and kept because the older docs
///   name it.
///
/// # Returns
///
/// Returns `Ok(())` if all required dependencies are present.
/// Returns an error if any required dependencies are missing.
pub(crate) async fn execute(probe: &dyn SystemProbePort, setup_fast_downloads: bool) -> Result<()> {
    println!("{}{}Checking system dependencies...{}\n", BOLD, INFO, RESET);

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
            SUCCESS, RESET, present_required, total_required
        );

        if setup_fast_downloads {
            println!(
                "{}Provisioning the hf_xet download accelerator...{}",
                BOLD, RESET
            );
            ensure_fast_helper_ready()
                .await
                .context("Failed to set up the hf_xet download accelerator")?;
            println!("{}✓ Download accelerator ready{}", SUCCESS, RESET);
        } else if !fast_helper_provisioned() {
            println!(
                "{}Downloads run natively over HTTP. To enable the optional hf_xet \
                 accelerator, run:{} gglib config fast-downloads enable",
                INFO, RESET
            );
        }

        print_gpu_status(probe);

        println!("\n{}You can now run: {}make setup{}", BOLD, INFO, RESET);
        Ok(())
    } else {
        println!(
            "{}✗ {} required dependencies are missing.{} ({}/{})",
            DANGER,
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
