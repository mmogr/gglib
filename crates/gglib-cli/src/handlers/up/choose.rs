//! Step 3: settle on a model, and get it onto the disk.
//!
//! Three ways in, in priority order: the user named one, the catalog already
//! has one, or nothing is installed and a recommendation has to be made and
//! confirmed.

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use gglib_app_services::download_budget;
use gglib_core::SettingsUpdate;
use gglib_core::domain::{Model, Recommendation, SizedAgainst, format_gib};

use super::{require_tty, row, sgr, step};
use crate::bootstrap::CliContext;
use crate::handlers::model::download::run_interactive_monitor;
use crate::handlers::model::resolver;
use crate::presentation::style::{BOLD, RESET, SUCCESS};
use crate::utils::input;

/// Resolve the model `up` will serve, downloading it if it isn't here yet.
pub(super) async fn run(ctx: &CliContext, requested: Option<&str>, yes: bool) -> Result<Model> {
    step(3, "Model");

    if let Some(identifier) = requested {
        let model = resolver::resolve_model_identifier(ctx, identifier).await?;
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

    download_recommended(ctx, yes).await
}

/// Make the model `up` serves the default when no default is set, and say
/// whether it did.
///
/// `gglib q` falls back to the default model, and a first run has none, so
/// without this the first question after `up` asks for `-m`. A default
/// the user chose is theirs, and stays; one whose model has since been
/// removed is as good as none.
pub(super) async fn adopt_as_default(ctx: &CliContext, model_id: i64) -> Result<bool> {
    if let Some(current) = ctx.app.settings().get().await?.default_model_id
        && ctx.app.models().get_by_id(current).await?.is_some()
    {
        return Ok(false);
    }
    let update = SettingsUpdate {
        default_model_id: Some(Some(model_id)),
        ..Default::default()
    };
    ctx.app.settings().update(update).await?;
    Ok(true)
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

/// Why nothing was suggested, and what to do: the load's budget it was
/// sized against, or that the load has none.
fn nothing_suggested(sized: Option<SizedAgainst>) -> String {
    let why = sized.map_or_else(
        || gglib_runtime::system::UNREADABLE_GPU.to_owned(),
        |sized| {
            format!(
                "No model in gglib's shortlist is served 8K of context or more in the {} a load \
                 sizes against.",
                sized.words()
            )
        },
    );
    format!("{why}\nDownload one yourself and re-run:\n  gglib model download <huggingface-repo>")
}

/// Nothing installed: recommend, explain, confirm, download.
async fn download_recommended(ctx: &CliContext, yes: bool) -> Result<Model> {
    // Sized as a load sizes a context: against the same reading of this
    // machine, by the same rule, with the user's `default-context-size`.
    let default_ctx = ctx.app.settings().get().await?.default_context_size;
    let Some(rec) = gglib_runtime::system::recommend_here(default_ctx) else {
        bail!(nothing_suggested(gglib_runtime::system::sized_against()));
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
    let quant = rec.candidate.quantization.map(str::to_owned);
    println!();
    Arc::clone(&ctx.downloads)
        .queue_smart(repo.clone(), quant, download_budget().await)
        .await?;
    run_interactive_monitor(Arc::clone(&ctx.downloads), Arc::clone(&ctx.console)).await?;

    // The monitor reports its own failures and still returns `Ok`, so the
    // catalog — not its return value — is what says whether this worked.
    find_downloaded(ctx, &repo).await
}

/// The rows under a recommendation, as label, value and note: the size its
/// download takes, and the load's own fit of that size on this machine.
///
/// The fit is the one a load makes (the same rule and the same budget), so
/// the context named is the one a load serves when nothing else is asked
/// for, and `gglib model explain` says the same sentence once it is down.
fn recommendation_rows(rec: &Recommendation) -> Vec<(&'static str, String, Option<String>)> {
    let c = rec.candidate;
    let mut rows = vec![(
        "size",
        format_gib(c.weights_bytes),
        c.quantization
            .map(|q| format!("{q}, the size it downloads")),
    )];
    if let Some(note) = &rec.fit.note {
        rows.push(("memory", note.clone(), None));
    }
    rows.push(("context", rec.fit.sentence.clone(), None));
    rows
}

/// State the choice and the arithmetic behind it.
///
/// The size and the memory are the part that earns trust: a bare model name
/// is a guess, whereas "5.29 GiB of 5.33 GiB usable" is a claim the user can
/// check.
fn print_recommendation(rec: &Recommendation) {
    let c = rec.candidate;
    println!();
    println!("  {}{}{}", sgr(BOLD), c.repo, sgr(RESET));
    println!("  {}", c.rationale);
    println!();
    for (label, value, note) in recommendation_rows(rec) {
        row(label, &value, note.as_deref());
    }
}

/// Ask, once it is established there is somebody to ask.
fn confirm() -> Result<bool> {
    require_tty("the download")?;
    println!();
    input::yes_no(
        "Download this model now?",
        false,
        input::Unasked::Refuse(input::nobody_to_ask(
            "nothing was downloaded",
            "--yes to download it",
        )),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::utils::system::SystemMemoryInfo;

    const GB: u64 = 1_073_741_824;

    async fn registered(ctx: &CliContext, dir: &std::path::Path, name: &str) -> i64 {
        let path = dir.join(format!("{name}.gguf"));
        let entry =
            gglib_core::domain::NewModel::new(name.to_owned(), path, Some(4.0), chrono::Utc::now());
        ctx.app.models().add(entry).await.expect("registered").id
    }

    /// The first model `up` serves becomes the default, so `gglib q` works
    /// straight after; a default already set is the user's and stays.
    #[tokio::test]
    async fn the_served_model_fills_an_empty_default_and_never_replaces_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = crate::bootstrap::test_context(dir.path()).await;
        let first = registered(&ctx, dir.path(), "first").await;
        let second = registered(&ctx, dir.path(), "second").await;

        assert!(adopt_as_default(&ctx, first).await.expect("first"));
        assert!(!adopt_as_default(&ctx, second).await.expect("second"));

        let settings = ctx.app.settings().get().await.expect("settings");
        assert_eq!(settings.default_model_id, Some(first));
    }

    /// A default whose model was removed leaves `gglib q` without a model,
    /// so `up` treats it as no default at all.
    #[tokio::test]
    async fn a_default_whose_model_is_gone_is_replaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = crate::bootstrap::test_context(dir.path()).await;
        let gone = registered(&ctx, dir.path(), "gone").await;
        let served = registered(&ctx, dir.path(), "served").await;
        assert!(adopt_as_default(&ctx, gone).await.expect("adopted"));
        ctx.app.models().delete(gone).await.expect("removed");

        assert!(adopt_as_default(&ctx, served).await.expect("replaced"));

        let settings = ctx.app.settings().get().await.expect("settings");
        assert_eq!(settings.default_model_id, Some(served));
    }

    const fn no_gpu(total_ram_bytes: u64) -> SystemMemoryInfo {
        SystemMemoryInfo {
            total_ram_bytes,
            gpu_memory_bytes: None,
            is_unified_memory: false,
            has_nvidia_gpu: false,
        }
    }

    /// The "nothing suggested" message quotes the budget the decision was
    /// made against, the load's, and with none says why and what to do.
    #[test]
    fn nothing_suggested_names_the_loads_budget_or_why_there_is_none() {
        let small = gglib_runtime::system::sized_against_on(false, &no_gpu(2 * GB));
        assert_eq!(
            nothing_suggested(small).lines().next(),
            Some(
                "No model in gglib's shortlist is served 8K of context or more in the 1.5 GiB \
                 of this machine's 2.0 GiB of RAM (no GPU) a load sizes against."
            )
        );
        let unread = nothing_suggested(None);
        assert!(
            unread.starts_with(gglib_runtime::system::UNREADABLE_GPU),
            "{unread}"
        );
        assert!(unread.ends_with("<huggingface-repo>"), "{unread}");
    }

    /// The rows name the size the download takes and say the load's own
    /// fit of it: the figures and the sentence a load of it says, not a
    /// second reckoning (ADR 0009 opens by condemning a number unrelated to
    /// the one served).
    #[test]
    fn the_rows_say_the_size_downloaded_and_the_loads_fit_of_it() {
        let vm = no_gpu(79 * GB / 10);
        let rec = gglib_runtime::system::recommend_on(false, &vm).expect("suggested");
        let rows = recommendation_rows(&rec);
        assert_eq!(
            rows,
            [
                (
                    "size",
                    format_gib(rec.candidate.weights_bytes),
                    Some("Q4_K_M, the size it downloads".to_owned())
                ),
                (
                    "memory",
                    rec.fit.note.clone().expect("a fit says its memory"),
                    None
                ),
                ("context", rec.fit.sentence.clone(), None),
            ]
        );
    }
}
