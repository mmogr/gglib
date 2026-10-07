//! Update command for llama.cpp.

use super::build_events::{BuildEvent, BuildPhase};
use super::config::recorded_build;
use super::detect::Acceleration;
use anyhow::{Context, Result, bail};
use gglib_core::paths::{llama_config_path, llama_cpp_dir, llama_server_path};
use gglib_core::utils::process::cmd;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// How far behind upstream the local llama.cpp checkout is.
///
/// Why the source checkout can be missing while a binary exists: the prebuilt
/// download path installs `llama-server` without ever cloning the repository,
/// and only a source install can be compared against upstream.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlamaUpdateCheck {
    pub installed: bool,
    /// Whether the llama.cpp source checkout exists to compare against.
    pub repo_present: bool,
    /// Whether a comparison actually happened. False means `commitsBehind` is
    /// 0 because nothing was compared, not because the checkout is current —
    /// the two are indistinguishable otherwise, and reporting the second when
    /// the first is true tells the user their install is up to date when we
    /// have no idea.
    pub comparable: bool,
    pub current_version: Option<String>,
    pub current_acceleration: Option<String>,
    pub build_date: Option<String>,
    pub commits_behind: u32,
    /// Up to five one-line summaries of what landed upstream.
    pub recent_commits: Vec<String>,
}

impl LlamaUpdateCheck {
    /// The states where no comparison is possible: nothing installed, or a
    /// prebuilt install with no source checkout, or no build's record to read.
    fn not_comparable(installed: bool, repo_present: bool) -> Self {
        Self {
            installed,
            repo_present,
            comparable: false,
            current_version: None,
            current_acceleration: None,
            build_date: None,
            commits_behind: 0,
            recent_commits: Vec::new(),
        }
    }
}

/// Compare the local llama.cpp checkout against upstream.
///
/// Network-bound: this runs `git fetch`, so it belongs behind an explicit
/// user action rather than a page load. Callers that only need what is
/// installed want [`super::llama_status`] instead, which is local and cheap.
pub async fn llama_update_check() -> Result<LlamaUpdateCheck> {
    let llama_dir = llama_cpp_dir()?;
    let binary_path = llama_server_path()?;

    if !binary_path.exists() {
        return Ok(LlamaUpdateCheck::not_comparable(false, false));
    }
    if !llama_dir.exists() {
        return Ok(LlamaUpdateCheck::not_comparable(true, false));
    }

    let Some(config) = recorded_build(&llama_config_path()?)? else {
        return Ok(LlamaUpdateCheck::not_comparable(true, true));
    };

    // `git fetch` is a network round-trip and every one of these is a
    // blocking subprocess, so the whole group moves off the async workers.
    let dir = llama_dir.clone();
    let (commits_behind, recent_commits) =
        tokio::task::spawn_blocking(move || fetch_and_count(&dir))
            .await
            .map_err(|e| anyhow::anyhow!("Update check task panicked: {e}"))??;

    Ok(LlamaUpdateCheck {
        installed: true,
        repo_present: true,
        comparable: true,
        current_version: Some(config.version),
        current_acceleration: Some(config.acceleration),
        build_date: Some(config.build_date.to_rfc3339()),
        commits_behind,
        recent_commits,
    })
}

/// Fetch from origin and count how far behind `HEAD` is. Blocking.
fn fetch_and_count(llama_dir: &std::path::Path) -> Result<(u32, Vec<String>)> {
    let dir_str = llama_dir
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("llama.cpp path is not valid UTF-8"))?;

    let status = cmd("git")
        .args(["-C", dir_str, "fetch", "origin"])
        .status()
        .context("Failed to fetch updates")?;

    if !status.success() {
        bail!("Failed to fetch updates from remote");
    }

    let output = cmd("git")
        .args(["-C", dir_str, "rev-list", "--count", "HEAD..origin/master"])
        .output()
        .context("Failed to check for updates")?;

    let commits_behind = parse_commits_behind(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )?;

    if commits_behind == 0 {
        return Ok((0, Vec::new()));
    }

    let output = cmd("git")
        .args([
            "-C",
            dir_str,
            "log",
            "--oneline",
            "-n",
            "5",
            "HEAD..origin/master",
        ])
        .output()
        .context("Failed to get commit log")?;

    let recent = String::from_utf8_lossy(&output.stdout)
        .lines()
        .take(5)
        .map(str::to_string)
        .collect();

    Ok((commits_behind, recent))
}

/// Turn `git rev-list --count`'s result into a commit count.
///
/// A non-zero exit means the comparison never happened — no `origin/master`,
/// a shallow clone, a corrupt checkout — and it must not be reported as zero.
/// [`LlamaUpdateCheck`] treats `commits_behind: 0` alongside `comparable:
/// true` as "up to date", the very distinction `not_comparable` exists to
/// preserve, so swallowing a failure here renders a broken repository as a
/// healthy one. An unparseable count on a *successful* exit is equally a
/// broken assumption rather than a zero.
fn parse_commits_behind(success: bool, stdout: &str, stderr: &str) -> Result<u32> {
    if !success {
        let detail = stderr.trim();
        if detail.is_empty() {
            bail!("Could not compare against origin/master");
        }
        bail!("Could not compare against origin/master: {detail}");
    }

    let raw = stdout.trim();
    raw.parse::<u32>()
        .with_context(|| format!("Unexpected `git rev-list --count` output: {raw:?}"))
}

/// Check for llama.cpp updates — a printer over [`llama_update_check`].
pub async fn handle_check_updates() -> Result<()> {
    // The header comes from local state, so print it before the fetch rather
    // than after: "Checking for updates..." should appear while the network
    // call is happening, not once it has already returned.
    let local_build = super::llama_status()
        .ok()
        .filter(|s| s.installed)
        .and_then(|s| s.build);

    if let Some(build) = local_build {
        let built = build.build_date.split('T').next().unwrap_or("unknown");
        println!("Current version: {} ({})", build.version, built);
        println!("Acceleration: {}", build.acceleration);
        println!();
        println!("Checking for updates...");
    }

    let check = llama_update_check().await?;

    if !check.installed {
        println!("llama.cpp is not installed.");
        println!("Run 'gglib config llama install' to install it.");
        return Ok(());
    }

    if !check.repo_present {
        println!("Warning: llama.cpp repository not found.");
        println!("Run 'gglib config llama rebuild' to reinstall.");
        return Ok(());
    }

    if check.current_version.is_none() {
        println!("Warning: Build configuration not found.");
        return Ok(());
    }

    if check.commits_behind == 0 {
        println!("✓ llama.cpp is up to date");
        return Ok(());
    }

    println!(
        "✓ New version available ({} commits ahead)",
        check.commits_behind
    );
    println!();
    println!("Recent changes:");
    for line in &check.recent_commits {
        println!("  {line}");
    }
    println!();
    println!("Run 'gglib config llama update' to upgrade");

    Ok(())
}

/// Pull upstream, then rebuild and reinstall — the shared update pipeline
/// behind `gglib config llama update` and the `system/update-llama` route.
///
/// Differs from [`run_llama_source_build`] only in the pull: that reuses an
/// existing checkout as-is, which is right for an install and wrong for an
/// update. Everything after the pull is the same pipeline, so an update
/// installs `llama-bench` alongside `llama-server` and reports progress.
///
/// [`run_llama_source_build`]: super::run_llama_source_build
pub async fn run_llama_update(
    acceleration: Acceleration,
    llama_dir: PathBuf,
    server_path: PathBuf,
    tx: mpsc::Sender<BuildEvent>,
) -> Result<()> {
    let dir = llama_dir.clone();
    let pull_tx = tx.clone();

    // `git pull` blocks, so it runs on a blocking thread. Its output is this
    // process's own: git reports a pull itself, and the CLI draws no spinner.
    tokio::task::spawn_blocking(move || -> Result<()> {
        let _ = pull_tx.blocking_send(BuildEvent::PhaseStarted {
            phase: BuildPhase::CloneOrUpdateRepo,
        });
        let _ = pull_tx.blocking_send(BuildEvent::Log {
            message: "Pulling latest llama.cpp changes...".to_string(),
        });

        let dir_str = dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("llama.cpp path is not valid UTF-8"))?;
        let status = cmd("git")
            .args(["-C", dir_str, "pull", "origin", "master"])
            .status()
            .context("Failed to pull updates")?;
        if !status.success() {
            bail!("Failed to pull updates from origin/master");
        }

        let _ = pull_tx.blocking_send(BuildEvent::PhaseCompleted {
            phase: BuildPhase::CloneOrUpdateRepo,
        });
        Ok(())
    })
    .await??;

    super::run_llama_source_build(acceleration, llama_dir, server_path, tx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `commitsBehind` of 0 with `repoPresent: false` means "cannot tell",
    /// not "up to date" — the GUI branches on `repoPresent` first for exactly
    /// this reason, so the distinction has to survive serialisation.
    #[test]
    fn not_comparable_is_distinct_from_up_to_date() {
        let prebuilt = LlamaUpdateCheck::not_comparable(true, false);
        assert!(prebuilt.installed);
        assert!(!prebuilt.repo_present);
        // The distinction the GUI depends on: 0 commits behind, but nothing
        // was compared, so it must not be shown as "up to date".
        assert!(!prebuilt.comparable);
        assert_eq!(prebuilt.commits_behind, 0);
        assert!(prebuilt.current_version.is_none());
    }

    /// The counterpart hazard to the one above, on the live path: a failed
    /// comparison read as zero commits behind would reach the caller with
    /// `comparable: true` — i.e. "up to date" — for a repository it could not
    /// read at all.
    #[test]
    fn failed_comparison_is_an_error_not_zero() {
        let err = parse_commits_behind(false, "", "fatal: bad revision 'origin/master'")
            .expect_err("a non-zero git exit must not report zero commits behind");
        assert!(
            err.to_string().contains("bad revision"),
            "the git failure should reach the caller: {err}"
        );
    }

    #[test]
    fn failed_comparison_without_stderr_still_errors() {
        assert!(parse_commits_behind(false, "", "   ").is_err());
    }

    /// A clean exit with output git never produces means the assumption
    /// behind the parse is wrong; that is not a zero either.
    #[test]
    fn unparseable_count_is_an_error() {
        assert!(parse_commits_behind(true, "not-a-number", "").is_err());
    }

    #[test]
    fn counts_parse_with_surrounding_whitespace() {
        assert_eq!(parse_commits_behind(true, "0\n", "").unwrap(), 0);
        assert_eq!(parse_commits_behind(true, "  42\n", "").unwrap(), 42);
    }

    #[test]
    fn update_check_serialises_as_camel_case() {
        let check = LlamaUpdateCheck {
            installed: true,
            repo_present: true,
            comparable: true,
            current_version: Some("abc1234".into()),
            current_acceleration: Some("Metal".into()),
            build_date: Some("2026-08-10T00:00:00Z".into()),
            commits_behind: 3,
            recent_commits: vec!["deadbee fix something".into()],
        };

        let json = serde_json::to_value(&check).unwrap();
        assert_eq!(json["repoPresent"], true);
        assert_eq!(json["comparable"], true);
        assert_eq!(json["currentVersion"], "abc1234");
        assert_eq!(json["currentAcceleration"], "Metal");
        assert_eq!(json["commitsBehind"], 3);
        assert_eq!(json["recentCommits"][0], "deadbee fix something");
        assert!(json.get("repo_present").is_none());
    }
}
