#![doc = include_str!("README.md")]

mod display;
mod instructions;
mod platform;

use anyhow::Result;
use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{BuildOrigin, Purpose};
use gglib_download::cli_exec::fast_helper_provisioned;

use display::{print_gpu_status, render_report};
use instructions::print_installation_instructions;

use crate::presentation::style::{BOLD, INFO, RESET};
use crate::presentation::terminal::stdout_width;

/// Execute the check-deps command.
///
/// Prints the dependency report, group by group: running gglib, building it
/// from source, building an engine from source. Fails while something this
/// gglib needs is missing, after saying how to install it. An installed copy
/// does not need the build groups, so their tools never fail it.
pub(crate) fn execute(probe: &dyn SystemProbePort) -> Result<()> {
    println!("{BOLD}{INFO}Checking system dependencies...{RESET}\n");

    let report = probe.dependency_report();
    print!("{}", render_report(&report, stdout_width()));

    let missing = report.missing();
    if !missing.is_empty() {
        print_installation_instructions(&missing);
        anyhow::bail!("{}", report.verdict())
    }

    if !fast_helper_provisioned() {
        println!(
            "{INFO}Downloads run natively over HTTP. To enable the optional hf_xet \
             accelerator, run:{RESET} gglib config fast-downloads enable"
        );
    }

    print_gpu_status(probe, report.needs(Purpose::BuildEngine));

    if report.origin == BuildOrigin::Checkout {
        println!("\n{BOLD}You can now run: {INFO}make setup{RESET}");
    }
    Ok(())
}
