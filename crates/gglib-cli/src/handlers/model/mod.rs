#![doc = include_str!("README.md")]
// MIGRATION: content extracted to README.md — remove this //! block after review
//! Model management command handlers.
//!
//! Dispatches [`ModelCommand`] variants to focused handler modules covering
//! CRUD, verification, download, and HuggingFace discovery.

pub mod add;
pub mod capabilities;
pub mod download;
pub mod inspect;
pub mod list;
pub mod remove;
pub mod resolver;
pub mod retag;
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
        ModelCommand::CheckUpdates { identifier, all } => {
            download::check_updates(ctx, identifier.as_deref(), all).await?;
        }
        ModelCommand::Upgrade {
            identifier,
            force: _force,
        } => {
            download::update_model(ctx, &identifier).await?;
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
    }
    Ok(())
}
