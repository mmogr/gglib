//! A product's source checkout, at the release its install selects.
//!
//! Every source build starts here. The checkout is a shallow clone of the
//! release's tag. One already at that tag is reused, so its build directory
//! survives. One at any other release is replaced, but only once the new one
//! has been built: the new checkout is cloned beside it and moves into place
//! when the caller [settles](Checkout::settle) it. Until then the old
//! checkout, and any installed binary that still loads libraries from it,
//! stays as it was. A checkout holding work of its own (a changed file, a
//! commit, a stash) is never replaced.

use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use tokio::sync::mpsc;

use gglib_core::utils::process::cmd;

use super::release::ReleaseSelector;
use crate::llama::{BuildEvent, BuildPhase};

/// Where both products' builds put their output inside the checkout.
const BUILD_DIR: &str = "build";

/// Where a product's source is cloned from, and how.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceRepo<'a> {
    /// How the product is named in messages (`llama.cpp`).
    pub(crate) product: &'a str,
    /// The URL `git clone` is given.
    pub(crate) url: &'a str,
    /// Whether the build needs the repository's submodules.
    pub(crate) submodules: bool,
}

/// A checkout ready to build.
#[derive(Debug)]
pub(crate) struct Checkout {
    /// Where to build: the product's checkout, or the new one beside it.
    pub(crate) dir: PathBuf,
    /// What the install records: the release's tag, or the short commit when
    /// the selector follows upstream's newest.
    pub(crate) version: String,
    /// The full commit checked out.
    pub(crate) commit_sha: String,
    /// The short commit checked out.
    pub(crate) short_sha: String,
    /// The tag checked out, when the selector named one.
    pub(crate) tag: Option<String>,
    /// The product's checkout, which [`Self::dir`] replaces when they differ.
    home: PathBuf,
}

impl Checkout {
    /// Move a new checkout into the product's place, retiring the old one.
    ///
    /// Call this once the build is installed: from then on nothing needs the
    /// old checkout. A checkout that was reused is already in place.
    ///
    /// The new checkout's build directory goes first. `CMake` records the
    /// folder it configured, so a build directory that moves is refused by
    /// the next configure; the binaries are installed and linked statically,
    /// so nothing needs it. The old checkout is checked again for work of its
    /// own, since a build takes long enough for some to appear, and is moved
    /// aside before the new one takes its name, and back again if the new one
    /// cannot. Removing the moved-aside one is the last step, and a failure
    /// there leaves it to the next install.
    pub(crate) fn settle(self) -> Result<()> {
        if self.dir == self.home {
            return Ok(());
        }
        if is_repository(&self.home) && has_own_work(&self.home)? {
            bail!(
                "{} gained changes of its own during the build, so it is left in place. The new \
                 build is installed; its checkout is at {}.",
                self.home.display(),
                self.dir.display()
            );
        }
        remove_if_present(&self.dir.join(BUILD_DIR))?;
        let retired = sibling(&self.home, "old");
        remove_if_present(&retired)?;
        if self.home.exists() {
            fs::rename(&self.home, &retired).with_context(|| {
                format!(
                    "Failed to move {} to {}",
                    self.home.display(),
                    retired.display()
                )
            })?;
        }
        if let Err(e) = fs::rename(&self.dir, &self.home) {
            if retired.exists() {
                let _ = fs::rename(&retired, &self.home);
            }
            return Err(e).with_context(|| {
                format!(
                    "Failed to move {} to {}",
                    self.dir.display(),
                    self.home.display()
                )
            });
        }
        let _ = fs::remove_dir_all(&retired);
        Ok(())
    }
}

/// Settle `checkout` once its build is installed, saying on `tx` why it could
/// not be when it could not: the install itself is done either way.
pub(crate) async fn settle(checkout: Checkout, tx: &mpsc::Sender<BuildEvent>) {
    let outcome = tokio::task::spawn_blocking(move || checkout.settle()).await;
    let problem = match outcome {
        Ok(Ok(())) => return,
        Ok(Err(e)) => format!("{e:#}"),
        Err(e) => format!("the task moving it ended early: {e}"),
    };
    let _ = tx
        .send(BuildEvent::Log {
            message: format!("Warning: the source checkout was not replaced: {problem}"),
        })
        .await;
}

/// The folders a checkout at `home` can leave beside it: a new checkout from
/// an install that did not finish, and an old one not yet removed. An
/// uninstall removes them with the checkout.
pub(crate) fn leftovers(home: &Path) -> [PathBuf; 2] {
    [sibling(home, "new"), sibling(home, "old")]
}

/// `home` with `.<suffix>` added to its name.
fn sibling(home: &Path, suffix: &str) -> PathBuf {
    let mut name = home.file_name().unwrap_or_default().to_owned();
    name.push(".");
    name.push(suffix);
    home.with_file_name(name)
}

fn remove_if_present(dir: &Path) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("Failed to remove {}", dir.display()))?;
    }
    Ok(())
}

/// `git clone`'s arguments for `selector` from `repo` into `dir`: shallow,
/// with the submodules when the build needs them, at the release's tag
/// unless following upstream's newest.
pub(crate) fn clone_args(repo: &SourceRepo, selector: &ReleaseSelector, dir: &Path) -> Vec<String> {
    let mut args = vec!["clone".to_owned(), "--depth".to_owned(), "1".to_owned()];
    if let ReleaseSelector::Tag(tag) = selector {
        args.extend(["--branch".to_owned(), tag.clone()]);
    }
    if repo.submodules {
        args.extend([
            "--recurse-submodules".to_owned(),
            "--shallow-submodules".to_owned(),
        ]);
    }
    args.extend([repo.url.to_owned(), dir.display().to_string()]);
    args
}

/// A checkout of `repo` at `home` for `selector`, cloning when the one there
/// is not at that release; blocking.
///
/// A checkout at `home` whose tags include the selector's is reused. Any
/// other is replaced by a clone into a directory beside it, which
/// [`Checkout::settle`] moves into place, unless it holds work of its own,
/// which is refused naming the folder. A selector that follows upstream's
/// newest never reuses one.
pub(crate) fn prepare_checkout(
    repo: &SourceRepo,
    selector: &ReleaseSelector,
    home: &Path,
    tx: &mpsc::Sender<BuildEvent>,
) -> Result<Checkout> {
    let dir = if home.exists() {
        if let ReleaseSelector::Tag(tag) = selector
            && tags_at_head(home)?.iter().any(|t| t == tag)
        {
            let _ = tx.blocking_send(BuildEvent::Log {
                message: format!("Using the existing {} checkout at {tag}.", repo.product),
            });
            home.to_path_buf()
        } else {
            if is_repository(home) && has_own_work(home)? {
                bail!(
                    "{} has changes of its own, so it is not replaced. Move them elsewhere or \
                     delete the folder, then try again.",
                    home.display()
                );
            }
            let staging = sibling(home, "new");
            clone(repo, selector, &staging, tx)?;
            staging
        }
    } else {
        clone(repo, selector, home, tx)?;
        home.to_path_buf()
    };

    let (short_sha, commit_sha) = head_commit(&dir)?;
    let tag = match selector {
        ReleaseSelector::Tag(tag) => Some(tag.clone()),
        ReleaseSelector::Latest => None,
    };
    Ok(Checkout {
        version: tag.clone().unwrap_or_else(|| short_sha.clone()),
        dir,
        commit_sha,
        short_sha,
        tag,
        home: home.to_path_buf(),
    })
}

/// Clone `repo` for `selector` into `dir`, replacing anything left there by
/// an attempt that did not finish; blocking.
fn clone(
    repo: &SourceRepo,
    selector: &ReleaseSelector,
    dir: &Path,
    tx: &mpsc::Sender<BuildEvent>,
) -> Result<()> {
    let _ = tx.blocking_send(BuildEvent::PhaseStarted {
        phase: BuildPhase::CloneOrUpdateRepo,
    });
    remove_if_present(dir)?;
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directory")?;
    }

    let mut child = git_command()
        .args(clone_args(repo, selector, dir))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to run git clone")?;
    if let Some(stderr) = child.stderr.take() {
        forward_git_progress(stderr, tx);
    }
    let status = child.wait().context("Failed to wait for git clone")?;
    if !status.success() {
        bail!("Failed to clone the {} repository", repo.product);
    }

    let _ = tx.blocking_send(BuildEvent::PhaseCompleted {
        phase: BuildPhase::CloneOrUpdateRepo,
    });
    Ok(())
}

/// Forward a git command's `stderr` to `tx` as [`BuildEvent::Log`] lines.
///
/// Git writes all progress to stderr. Read on an OS thread (blocking I/O).
/// Carriage-return progress lines (e.g. "Receiving objects: 45%\r") are
/// filtered: `BufRead::lines()` keeps \r as trailing content; any line
/// containing \r is dropped to avoid corrupting SSE streams.
fn forward_git_progress(stderr: std::process::ChildStderr, tx: &mpsc::Sender<BuildEvent>) {
    let tx_reader = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() || line.contains('\r') {
                continue;
            }
            if tx_reader
                .blocking_send(BuildEvent::Log { message: line })
                .is_err()
            {
                break;
            }
        }
    });
}

/// `git`, deaf to the repository variables a caller's environment may carry:
/// run from a git hook, `GIT_DIR` would point every command here at the
/// hook's repository instead of the checkout.
fn git_command() -> Command {
    let mut command = cmd("git");
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
    ] {
        command.env_remove(var);
    }
    command
}

/// The output of `git -C <dir> <args>`, trimmed; `Err` when git fails.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = git_command()
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Whether `dir` is a repository of its own. Asked before any git command
/// that judges it: a folder that is not one, inside one that is, would be
/// answered for by the outer repository.
fn is_repository(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Every tag that points at the checkout's `HEAD`; none for a folder that is
/// not a repository.
///
/// More than one can: llama.cpp's `v*` releases point at the commit of the
/// `b<n>` build they name.
pub(crate) fn tags_at_head(dir: &Path) -> Result<Vec<String>> {
    if !is_repository(dir) {
        return Ok(Vec::new());
    }
    Ok(git(dir, &["tag", "--points-at", "HEAD"])?
        .lines()
        .map(str::to_owned)
        .collect())
}

/// Whether the checkout holds work that a replacement would lose: a change
/// to a tracked file, a stash, or a commit, on `HEAD` or a local branch,
/// that no remote branch or tag holds. Build output and other untracked
/// files do not count.
fn has_own_work(dir: &Path) -> Result<bool> {
    let changed = !git(dir, &["status", "--porcelain", "--untracked-files=no"])?.is_empty();
    let stashed = !git(dir, &["stash", "list"])?.is_empty();
    let committed = !git(
        dir,
        &[
            "rev-list",
            "--max-count=1",
            "HEAD",
            "--branches",
            "--not",
            "--remotes",
            "--tags",
        ],
    )?
    .is_empty();
    Ok(changed || stashed || committed)
}

/// The checkout's `HEAD`, short and full.
fn head_commit(dir: &Path) -> Result<(String, String)> {
    Ok((
        git(dir, &["rev-parse", "--short", "HEAD"])?,
        git(dir, &["rev-parse", "HEAD"])?,
    ))
}

#[cfg(all(test, unix))]
#[path = "checkout_tests.rs"]
mod tests;
