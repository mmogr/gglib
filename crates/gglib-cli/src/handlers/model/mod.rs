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
pub(crate) mod update;
pub(crate) mod verification;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::model_commands::ModelCommand;

/// Dispatch a `model` subcommand to its handler.
pub(crate) async fn dispatch(ctx: &CliContext, command: ModelCommand) -> Result<()> {
    match command {
        ModelCommand::Add { file_path } => {
            add::execute(ctx, &file_path).await?;
        }
        ModelCommand::List {
            sort,
            order,
            min_params,
            max_params,
            min_speed,
            max_speed,
            tags,
        } => {
            list::execute(
                ctx,
                list::ListArgs {
                    sort,
                    order,
                    min_params,
                    max_params,
                    min_speed,
                    max_speed,
                    tags,
                },
            )
            .await?;
        }
        ModelCommand::Remove { identifier, force } => {
            remove::execute(ctx, &identifier, force).await?;
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
            clear_inference_defaults,
            dry_run,
            force,
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
                clear_inference_defaults,
                dry_run,
                force,
            };
            update::execute(ctx, args).await?;
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
            verbose,
        } => {
            verification::execute_verify(ctx, &identifier, verbose).await?;
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
            // `--force` skips a confirmation prompt, and this path has none to
            // skip: the daemon owns the download and `exec` never asks. The
            // flag is inert, like `--skip-db` above it but without the notice.
            // Bound and dropped here rather than threaded into `DownloadArgs`,
            // where it was carried three layers and read by nobody.
            force: _,
        } => {
            // Registration happens daemon-side as a queue lifecycle phase, so
            // honouring this flag needs a protocol change. Say so rather than
            // registering silently — but do not refuse the download: the flag
            // was already a no-op, and failing here would break invocations
            // that used to work. `--list-quants` never touched the database,
            // so it is not worth a warning at all.
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
            inspect::execute(ctx, &identifier, metadata, json).await?;
        }
        ModelCommand::Explain {
            identifier,
            profile,
        } => {
            explain::execute(ctx, &identifier, profile.as_deref()).await?;
        }
    }
    Ok(())
}
