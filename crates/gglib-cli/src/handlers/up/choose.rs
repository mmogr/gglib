//! Step 3: settle on a model, and get it onto the disk.
//!
//! Three ways in, in priority order: the user named one, the catalog already
//! has one, or nothing is installed and a recommendation has to be made and
//! confirmed.

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use gglib_core::domain::{Model, Recommendation, format_gib, recommend};
use gglib_core::utils::system::SystemMemoryInfo;

use super::{require_tty, row, sgr, step};
use crate::bootstrap::CliContext;
use crate::handlers::model::download::run_interactive_monitor;
use crate::presentation::style::{BOLD, RESET, SUCCESS};
use crate::utils::input;

/// Resolve the model `up` will serve, downloading it if it isn't here yet.
pub(super) async fn run(
    ctx: &CliContext,
    memory: &Option<SystemMemoryInfo>,
    requested: Option<&str>,
    yes: bool,
) -> Result<Model> {
    step(3, "Model");

    if let Some(identifier) = requested {
        let model = ctx.app.models().find_by_identifier(identifier).await?;
        row("model", &model.name, Some("requested with --model"));
        return Ok(model);
    }

    if let Some(model) = most_recent(ctx).await? {
        row(
            "model",
            &model.name,
            Some("already installed; --model picks another"),
        );
        return Ok(model);
    }

    download_recommended(ctx, memory, yes).await
}

/// The most recently added model, if the catalog has anything at all.
///
/// Recency is the only ordering that means something here: on a re-run it is
/// whatever the previous run installed, and on a long-lived install it is the
/// model the user most recently went to the trouble of fetching.
async fn most_recent(ctx: &CliContext) -> Result<Option<Model>> {
    let mut models = ctx.app.models().list().await?;
    models.sort_by_key(|m| m.added_at);
    Ok(models.pop())
}

/// Nothing installed: recommend, explain, confirm, download.
async fn download_recommended(
    ctx: &CliContext,
    memory: &Option<SystemMemoryInfo>,
    yes: bool,
) -> Result<Model> {
    let Some(memory) = memory else {
        bail!(
            "Could not read this system's memory, so there is no safe way to pick a model.\n\
             Download one yourself and re-run:\n  \
             gglib model download <huggingface-repo>"
        );
    };

    let Some(rec) = recommend(memory) else {
        bail!(
            "No model in gglib's shortlist fits {} of {}.\n\
             Pick something smaller yourself and re-run:\n  \
             gglib model download <huggingface-repo>",
            format_gib(budget_of(memory)),
            source_label(memory),
        );
    };

    print_recommendation(&rec);

    if !yes && !confirm()? {
        bail!("Cancelled. Nothing was downloaded.");
    }

    // The user has just agreed to fetch several gigabytes, which is the moment
    // the accelerator is worth the interruption and no earlier. Skipped
    // entirely under `--yes`: that flag authorises the download, not building
    // a Python environment, and the offer would have nobody to answer it.
    if !yes {
        offer_fast_downloads().await;
    }

    let repo = rec.candidate.repo.to_string();
    let quant = rec.candidate.quantization.to_string();
    println!();
    Arc::clone(&ctx.downloads)
        .queue_smart(repo.clone(), Some(quant))
        .await?;
    run_interactive_monitor(
        Arc::clone(&ctx.downloads),
        Arc::clone(&ctx.download_emitter),
    )
    .await?;

    // The monitor reports its own failures and still returns `Ok`, so the
    // catalog — not its return value — is what says whether this worked.
    find_downloaded(ctx, &repo).await
}

/// What the `context` row says, since it cannot say a number.
const SERVED_CONTEXT: &str = "sized at launch";

/// And where that size comes from, so "sized at launch" is not merely a shrug.
const SERVED_CONTEXT_NOTE: &str = "from this machine's free memory";

/// The note beside `needs`, naming what `context` was used *for*.
///
/// "to clear", not "at": the shortlist tested this model against that context
/// to decide whether to offer it, which is not the same as serving it there.
fn needs_note(context: u64) -> String {
    format!("to clear {context} context")
}

/// State the choice and the arithmetic behind it.
///
/// The size and headroom are the part that earns trust: a bare model name is a
/// guess, whereas "18.9 GiB of 24.0 GiB VRAM" is a claim the user can check.
///
/// `c.context` is the bar this model had to clear to be shortlisted, not a
/// promise about the launch. The two are different questions — the shortlist
/// asks "does this fit *at* 32k", `fit_context` asks "what is the largest rung
/// this fits" — and they are answered against different budgets, so predicting
/// one from the other here would be a new false claim on the surface ADR 0009
/// opens by condemning for making one. `up` sends `None` and the daemon
/// decides; the note says so rather than naming a number.
fn print_recommendation(rec: &Recommendation) {
    let c = rec.candidate;
    println!();
    println!(
        "  {}{}{}  {}",
        sgr(BOLD),
        c.repo,
        sgr(RESET),
        c.quantization
    );
    println!("  {}", c.rationale);
    println!();
    row("size", &format_gib(c.weights_bytes), Some("download"));
    row(
        "needs",
        &format!(
            "{} of {} {}",
            format_gib(c.required_bytes()),
            format_gib(rec.budget_bytes),
            rec.budget_source.label(),
        ),
        Some(&needs_note(c.context)),
    );
    row("spare", &format_gib(rec.headroom_bytes), None);
    row("context", SERVED_CONTEXT, Some(SERVED_CONTEXT_NOTE));
}

/// Ask, once it is established there is somebody to ask.
fn confirm() -> Result<bool> {
    require_tty("the download")?;
    println!();
    input::prompt_confirmation("Download this model now?")
}

/// Offer the download accelerator before the first download runs.
///
/// `up` is the path a user who installed a prebuilt binary takes, and they
/// never see `make setup`. Without this the accelerator would stay invisible
/// to almost everybody who is not building from a clone.
///
/// Infallible by construction: this is an optional extra offered in the middle
/// of `up`'s five steps, and nothing it can do — declining, no Python, a
/// failed install — is a reason not to go on and download the model.
async fn offer_fast_downloads() {
    if let Err(e) = crate::handlers::config::fast_downloads::dispatch(Some(
        crate::config_commands::FastDownloadsCommand::Prompt,
    ))
    .await
    {
        tracing::debug!(error = %e, "skipping the fast-download offer");
    }
}

/// Locate the model the download registered.
///
/// Matching on the repository id rather than a name is deliberate: the
/// registrar derives display names from GGUF metadata, so the name is not
/// knowable from the shortlist entry, but the repository is exactly what was
/// queued.
async fn find_downloaded(ctx: &CliContext, repo: &str) -> Result<Model> {
    ctx.app
        .models()
        .list()
        .await?
        .into_iter()
        .find(|m| m.hf_repo_id.as_deref() == Some(repo))
        .ok_or_else(|| {
            anyhow!(
                "{repo} did not finish downloading — see the errors above.\n\
                 Re-run `gglib up` to try again, or fetch it directly with:\n  \
                 gglib model download {repo}"
            )
        })
        .inspect(|m| {
            println!();
            println!(
                "  {}\u{2713}{} {} is ready",
                sgr(SUCCESS),
                sgr(RESET),
                m.name
            );
        })
}

/// The memory figure a recommendation would have been sized against, for the
/// message that explains why there wasn't one.
fn budget_of(mem: &SystemMemoryInfo) -> u64 {
    mem.gpu_memory_bytes.unwrap_or(mem.total_ram_bytes)
}

fn source_label(mem: &SystemMemoryInfo) -> &'static str {
    match mem.gpu_memory_bytes {
        Some(_) if mem.is_unified_memory => "unified memory",
        Some(_) => "VRAM",
        None => "system RAM",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_073_741_824;

    fn mem(vram: Option<u64>, apple: bool) -> SystemMemoryInfo {
        SystemMemoryInfo {
            total_ram_bytes: 32 * GB,
            gpu_memory_bytes: vram,
            is_unified_memory: apple,
            has_nvidia_gpu: vram.is_some() && !apple,
        }
    }

    /// The "nothing fits" message quotes a budget, and quoting the wrong one
    /// would send the user looking at the wrong number.
    #[test]
    fn budget_prefers_vram_and_falls_back_to_ram() {
        assert_eq!(budget_of(&mem(Some(8 * GB), false)), 8 * GB);
        assert_eq!(budget_of(&mem(None, false)), 32 * GB);
    }

    /// The banner used to annotate `needs` with "at 32768 context", which reads
    /// as the context the launch will serve. It is not. That number is the bar
    /// the model had to clear to be shortlisted, tested against a different
    /// budget from the one `fit_context` uses — and `up` sends `None`, so the
    /// daemon decides. ADR 0009 opens by condemning exactly this surface for
    /// showing a number unrelated to the one served.
    #[test]
    fn the_needs_note_does_not_promise_a_served_context() {
        let note = needs_note(32_768);
        assert!(note.contains("32768"), "the bar is still stated: {note}");
        assert!(
            !note.starts_with("at "),
            "must not read as the served context: {note}"
        );
    }

    /// And the row that does speak for the launch names no number at all,
    /// because none is knowable here.
    #[test]
    fn the_context_row_names_no_number() {
        assert!(
            !SERVED_CONTEXT.chars().any(|c| c.is_ascii_digit()),
            "a number here would be a guess: {SERVED_CONTEXT}"
        );
        assert!(!SERVED_CONTEXT_NOTE.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn source_label_names_the_pool_the_budget_came_from() {
        assert_eq!(source_label(&mem(Some(8 * GB), false)), "VRAM");
        assert_eq!(source_label(&mem(Some(8 * GB), true)), "unified memory");
        assert_eq!(source_label(&mem(None, false)), "system RAM");
    }
}
