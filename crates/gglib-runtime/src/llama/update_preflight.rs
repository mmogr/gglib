//! Whether an update may start: one rule, asked by every surface.
//!
//! `gglib config llama update` and the `system/update-llama` route both call
//! [`update_preflight`] before anything is pulled, and both show a refusal as
//! it is worded here. One install therefore gets one verdict and one remedy,
//! whichever surface asked.

use std::path::{Path, PathBuf};

use anyhow::Result;
use gglib_core::paths::{llama_config_path, llama_cpp_dir, llama_server_path};
use gglib_core::utils::process::cmd;

use super::config::{BuildConfig, recorded_build};
use super::deps::{build_tool_install_lines, build_tools, missing_build_tools};
use super::detect::{Acceleration, detect_optimal_acceleration};

/// What an update will work on, once the preflight has let it start.
#[derive(Debug, Clone)]
pub struct UpdatePlan {
    /// What to rebuild with: the acceleration the current build recorded, so
    /// that an update never changes backend, or what is detected when there
    /// is no record.
    pub acceleration: Acceleration,
    /// The source checkout the update pulls into.
    pub llama_dir: PathBuf,
    /// Where the rebuilt `llama-server` is installed.
    pub server_path: PathBuf,
    /// The build being replaced, when it left a record.
    pub recorded: Option<BuildConfig>,
    /// Something to tell the user before the update runs. It does not stop
    /// the update.
    pub caution: Option<String>,
}

/// Why an update may not start, worded for the user with what to do instead.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateRefusal {
    /// There is no `llama-server` to update.
    #[error("llama.cpp is not installed.\nRun 'gglib config llama install' to install it.")]
    NotInstalled,

    /// A binary with no checkout beside it, which is what a pre-built
    /// download leaves. Only a build from source makes one, and `rebuild` is
    /// the one command that always builds.
    #[error(
        "There is no llama.cpp source checkout to update, as after a pre-built download.\n\
         Run 'gglib config llama rebuild' to build llama.cpp from source; an update works after that."
    )]
    NoSourceCheckout,

    /// The build that follows the pull could not run. Holds the tools that
    /// were not found.
    #[error(
        "An update rebuilds llama.cpp from source, and these build tools were not found: {}.\n{}\n\
         Install them, then update again.",
        .0.join(", "),
        build_tool_install_lines().join("\n")
    )]
    MissingTools(Vec<&'static str>),
}

/// What [`UpdatePlan::caution`] says of a checkout with local changes.
///
/// A caution and not a refusal: git pulls over changes upstream did not
/// touch, so an update of a patched checkout can work.
pub const LOCAL_CHANGES_CAUTION: &str = "The llama.cpp checkout has local changes. The update keeps them, and stops \
     at the pull if upstream changed the same files.";

/// What the preflight looked at, apart from how it looked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct UpdateFacts {
    /// `llama-server` is where the launcher runs it from.
    binary_installed: bool,
    /// The llama.cpp source checkout is there.
    checkout_present: bool,
    /// The build tools that were not found on `PATH`.
    missing_tools: Vec<&'static str>,
    /// The checkout has changes to files git tracks.
    local_changes: bool,
}

/// The rule: the first thing wrong refuses the update, in the order a user
/// would have to put them right. `Ok` holds the caution, if there is one.
fn judge(facts: &UpdateFacts) -> Result<Option<&'static str>, UpdateRefusal> {
    if !facts.binary_installed {
        return Err(UpdateRefusal::NotInstalled);
    }
    if !facts.checkout_present {
        return Err(UpdateRefusal::NoSourceCheckout);
    }
    if !facts.missing_tools.is_empty() {
        return Err(UpdateRefusal::MissingTools(facts.missing_tools.clone()));
    }
    Ok(facts.local_changes.then_some(LOCAL_CHANGES_CAUTION))
}

/// Look at the install and decide whether an update may start.
///
/// A refusal is an [`UpdateRefusal`] inside the error, and its message is the
/// whole of what to show. Blocking: it runs the build tools and git to see
/// that they are there.
pub fn update_preflight() -> Result<UpdatePlan> {
    let llama_dir = llama_cpp_dir()?;
    let server_path = llama_server_path()?;

    let checkout_present = llama_dir.exists();
    let facts = UpdateFacts {
        binary_installed: server_path.exists(),
        checkout_present,
        missing_tools: missing_build_tools(&build_tools()),
        local_changes: checkout_present && has_local_changes(&llama_dir),
    };
    let caution = judge(&facts)?;

    let recorded = recorded_build(&llama_config_path()?)?;
    Ok(UpdatePlan {
        acceleration: acceleration_for(recorded.as_ref())?,
        llama_dir,
        server_path,
        recorded,
        caution: caution.map(str::to_string),
    })
}

/// The acceleration an update should rebuild with.
///
/// Whatever the current build recorded, so an update never silently changes
/// backend; detection only decides when there is no record or the recorded
/// name is not one we build for. Detection is deliberately fallible — it
/// refuses to fall back to CPU — so this can fail with the install hints.
fn acceleration_for(recorded: Option<&BuildConfig>) -> Result<Acceleration> {
    Ok(match recorded.map(|c| c.acceleration.as_str()) {
        Some("Metal") => Acceleration::Metal,
        Some("CUDA") => Acceleration::Cuda,
        Some("Vulkan") => Acceleration::Vulkan,
        _ => detect_optimal_acceleration()?,
    })
}

/// Whether the checkout has changes to files git tracks.
///
/// Asked only of a directory that is a repository itself, since git would
/// otherwise answer for whichever repository the directory sits inside. A
/// checkout git cannot read counts as having none: the pull is what will say
/// what is wrong with it.
fn has_local_changes(llama_dir: &Path) -> bool {
    llama_dir.join(".git").exists()
        && cmd("git")
            .arg("-C")
            .arg(llama_dir)
            .args(["status", "--porcelain", "--untracked-files=no"])
            .output()
            .is_ok_and(|output| output.status.success() && !output.stdout.trim_ascii().is_empty())
}

#[cfg(test)]
#[path = "update_preflight_tests.rs"]
mod tests;
