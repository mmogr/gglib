#![doc = include_str!("README.md")]

mod display;
mod instructions;
mod platform;

use anyhow::Result;
use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyStatus};
use gglib_download::cli_exec::fast_helper_provisioned;

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
///
/// # Returns
///
/// Returns `Ok(())` if all required dependencies are present.
/// Returns an error if any required dependencies are missing.
pub(crate) fn execute(probe: &dyn SystemProbePort) -> Result<()> {
    println!("{BOLD}{INFO}Checking system dependencies...{RESET}\n");

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
            "{SUCCESS}✓ All required dependencies are installed!{RESET} ({present_required}/{total_required})"
        );

        if !fast_helper_provisioned() {
            println!(
                "{INFO}Downloads run natively over HTTP. To enable the optional hf_xet \
                 accelerator, run:{RESET} gglib config fast-downloads enable"
            );
        }

        print_gpu_status(probe);

        println!("\n{BOLD}You can now run: {INFO}make setup{RESET}");
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
