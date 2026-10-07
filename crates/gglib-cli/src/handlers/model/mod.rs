#![doc = include_str!("README.md")]
pub(crate) mod add;
pub(crate) mod capabilities;
pub(crate) mod download;
pub(crate) mod explain;
pub(crate) mod inspect;
pub(crate) mod list;
pub(crate) mod remove;
pub(crate) mod resolver;
pub(crate) mod retag;
#[allow(
    clippy::assigning_clones,
    clippy::option_if_let_else,
    clippy::ref_option,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unnecessary_wraps,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod update;
mod update_projector;
pub(crate) mod verification;

#[cfg(test)]
mod test_library;

use std::sync::Arc;

use anyhow::Result;
use gglib_app_services::{ModelDeps, ModelOps};

use crate::bootstrap::CliContext;
use crate::model_commands::ModelCommand;
use crate::target::Target;

/// `ModelOps` for a one-shot CLI command.
///
/// A listing, an explanation, an edit and a removal made in a terminal go
/// through it, so each runs the operation the GUI's runs, as a capability
/// change and an upgrade do.
/// The reasons for what it is built with live here once:
///
/// - `NoopModelRuntime` rather than `ctx.runner`: a one-shot command has no
///   shared `ProcessManager` to consult, and a runner scoped to this single
///   invocation could only ever answer "nothing is running". So the refusal
///   to remove a model that is being served never fires from here: what the
///   daemon is serving is the daemon's to know.
/// - `NoopEmitter`: library events exist to tell *other* clients what changed.
///   A CLI process that is about to exit has no broadcast channel to tell
///   them on, so the events `ModelOps` emits end here.
pub(crate) fn one_shot_model_ops(ctx: &CliContext) -> ModelOps {
    ModelOps::new(ModelDeps {
        core: ctx.app.clone(),
        runtime: Arc::new(gglib_core::ports::NoopModelRuntime),
        gguf_parser: ctx.gguf_parser.clone(),
        emitter: Arc::new(gglib_core::ports::NoopEmitter::new()),
    })
}

/// Dispatch a `model` subcommand to its handler.
pub(crate) async fn dispatch(
    ctx: &CliContext,
    command: ModelCommand,
    target: Target,
) -> Result<()> {
    dispatch_with(ctx, &one_shot_model_ops(ctx), command, target).await
}

/// [`dispatch`], with the `ModelOps` a command reads and writes the library
/// through, so a test can watch what it does with them.
#[allow(
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
async fn dispatch_with(
    ctx: &CliContext,
    ops: &ModelOps,
    command: ModelCommand,
    target: Target,
) -> Result<()> {
    match command {
        ModelCommand::Add {
            file_path,
            reimport,
        } => {
            add::execute(ctx, &file_path, reimport).await?;
        }
        ModelCommand::List(args) => {
            list::execute(target, ctx, ops, args).await?;
        }
        ModelCommand::Remove { identifier, force } => {
            remove::execute(ctx, ops, &identifier, force).await?;
        }
        ModelCommand::Update {
            identifier,
            name,
            param_count,
            architecture,
            quantization,
            context_length,
            metadata,
            remove_metadata,
            replace_metadata,
            temperature,
            top_p,
            top_k,
            max_tokens,
            repeat_penalty,
            presence_penalty,
            min_p,
            dry_multiplier,
            dry_base,
            dry_allowed_length,
            dry_penalty_last_n,
            dynatemp_range,
            dynatemp_exponent,
            top_n_sigma,
            frequency_penalty,
            reasoning_effort,
            reasoning_budget_tokens,
            unset,
            clear_inference_defaults,
            dry_run,
            force,
            projector,
        } => {
            let args = update::UpdateArgs {
                identifier,
                name,
                param_count,
                architecture,
                quantization,
                context_length,
                metadata,
                remove_metadata,
                replace_metadata,
                temperature,
                top_p,
                top_k,
                max_tokens,
                repeat_penalty,
                presence_penalty,
                min_p,
                dry_multiplier,
                dry_base,
                dry_allowed_length,
                dry_penalty_last_n,
                dynatemp_range,
                dynatemp_exponent,
                top_n_sigma,
                frequency_penalty,
                reasoning_effort,
                reasoning_budget_tokens,
                unset,
                clear_inference_defaults,
                dry_run,
                force,
                projector,
            };
            update::execute(ctx, ops, args).await?;
        }
        ModelCommand::Retag {
            identifier,
            all,
            full,
        } => {
            retag::execute(ctx, identifier, all, full).await?;
        }
        ModelCommand::Verify {
            identifier,
            per_shard,
        } => {
            verification::execute_verify(ctx, &identifier, per_shard).await?;
        }
        ModelCommand::Repair {
            identifier,
            shards,
            force,
        } => {
            verification::execute_repair(ctx, &identifier, shards, force).await?;
        }
        ModelCommand::Download {
            model_id,
            quantization,
            list_quants,
            skip_db,
            token,
        } => {
            // Registration happens daemon-side as a queue lifecycle phase, so
            // honouring this flag needs a protocol change. Say so rather than
            // registering silently — but do not refuse the download: failing
            // here would break invocations that otherwise download fine.
            // `--list-quants` never touches the database, so it is not worth
            // a warning at all.
            if skip_db && !list_quants {
                eprintln!(
                    "warning: --skip-db is not currently honoured — downloads register through \
                     the daemon queue. The download will proceed and the model will be \
                     registered; `gglib model remove <id>` drops the row and keeps the file."
                );
            }
            let args = download::DownloadArgs {
                model_id: &model_id,
                quantization: quantization.as_deref(),
                list_quants,
                token: token.as_deref(),
            };
            download::download(ctx, args).await?;
        }
        ModelCommand::CheckUpdates { identifier, all } => {
            download::check_updates(ctx, identifier.as_deref(), all).await?;
        }
        ModelCommand::Upgrade { identifier, force } => {
            download::update_model(ctx, &identifier, force).await?;
        }
        ModelCommand::Search {
            query,
            limit,
            sort,
            gguf_only,
        } => {
            download::search(query, limit, sort, gguf_only).await?;
        }
        ModelCommand::Browse {
            category,
            limit,
            size,
        } => {
            download::browse(category, limit, size).await?;
        }
        ModelCommand::Capabilities {
            identifier,
            set,
            unset,
        } => {
            capabilities::execute(ctx, &identifier, set, unset).await?;
        }
        ModelCommand::Inspect {
            identifier,
            metadata,
            json,
        } => {
            inspect::execute(ctx, target, &identifier, metadata, json).await?;
        }
        ModelCommand::Explain {
            identifier,
            profile,
        } => {
            explain::execute(ctx, ops, &identifier, profile.as_deref()).await?;
        }
    }
    Ok(())
}
