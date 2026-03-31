//! Model management command handlers.
//!
//! Dispatches [`ModelCommand`] variants to focused handler modules covering
//! CRUD, verification, download, and HuggingFace discovery.

pub mod add;
pub mod download;
pub mod list;
pub mod remove;
pub mod update;
pub mod verification;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::model_commands::ModelCommand;

/// Dispatch a `model` subcommand to its handler.
pub async fn dispatch(ctx: &CliContext, command: ModelCommand) -> Result<()> {
    match command {
        ModelCommand::Add { file_path } => {
            add::execute(ctx, &file_path).await?;
        }
        ModelCommand::List => {
            list::execute(ctx).await?;
        }
        ModelCommand::Remove { identifier, force } => {
            remove::execute(ctx, &identifier, force).await?;
        }
        ModelCommand::Update {
            id,
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
            clear_inference_defaults,
            dry_run,
            force,
        } => {
            let args = update::UpdateArgs {
                id,
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
                clear_inference_defaults,
                dry_run,
                force,
            };
            update::execute(ctx, args).await?;
        }
        ModelCommand::Verify { model_id, verbose } => {
            verification::execute_verify(ctx, model_id, verbose).await?;
        }
        ModelCommand::Repair {
            model_id,
            shards,
            force,
        } => {
            verification::execute_repair(ctx, model_id, shards, force).await?;
        }
        ModelCommand::Download {
            model_id,
            quantization,
            list_quants,
            skip_db: _skip_db,
            token,
            force,
        } => {
            let args = download::DownloadArgs {
                model_id: &model_id,
                quantization: quantization.as_deref(),
                list_quants,
                force,
                token: token.as_deref(),
            };
            download::download(ctx, args).await?;
        }
        ModelCommand::CheckUpdates { model_id, all } => {
            download::check_updates(ctx, model_id, all).await?;
        }
        ModelCommand::Upgrade {
            model_id,
            force: _force,
        } => {
            download::update_model(ctx, model_id).await?;
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
    }
    Ok(())
}
