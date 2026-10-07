//! Upgrading a model to its repository's latest revision: the check, and
//! the download that follows it.
//!
//! One implementation for `gglib model upgrade` and the daemon's routes.

use std::future::Future;
use std::sync::Arc;

use gglib_core::events::AppEvent;
use gglib_download::cli_exec::{
    CliDownloadResult, CliUpdateRequest, RowCallback, UpdateCheckResult,
};

use crate::error::GuiError;
use crate::models::ModelOps;
use crate::types::{UpgradeCheck, UpgradeOutcome};

impl ModelOps {
    /// Preconditions shared by the upgrade check and the upgrade itself.
    fn upgrade_source(model: &gglib_core::Model) -> Result<(String, String), GuiError> {
        let repo = model.hf_repo_id.clone().ok_or_else(|| {
            GuiError::ValidationFailed("Model is not from HuggingFace, cannot update".into())
        })?;
        let quant = model.quantization.clone().ok_or_else(|| {
            GuiError::ValidationFailed("Model has no quantization info stored".into())
        })?;
        Ok((repo, quant))
    }

    /// Whether a newer `HuggingFace` revision exists — the commit-SHA check
    /// `gglib model upgrade` runs before downloading, distinct from the
    /// shard-level diff on `/{id}/updates`.
    ///
    /// Not the same question as `gglib model check-updates`: with no recorded
    /// revision this reports `has_update: true` (nothing to compare against)
    /// where that command declines to answer. Callers should present a
    /// `current_sha` of `None` as "no baseline recorded", not as a new release.
    pub async fn check_upgrade(&self, id: i64) -> Result<UpgradeCheck, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let (repo, _quant) = Self::upgrade_source(&model)?;

        let check = gglib_download::cli_exec::check_update(
            &repo,
            model.hf_commit_sha.as_deref(),
            std::env::var("HF_TOKEN").ok(),
        )
        .await
        .map_err(|e| GuiError::Internal(format!("Update check failed: {e}")))?;

        Ok(UpgradeCheck {
            has_update: check.has_update,
            current_sha: check.current_sha,
            latest_sha: check.latest_sha,
        })
    }

    /// Re-download the model at the latest `HuggingFace` revision and rewrite
    /// the row — `gglib model upgrade`, shared by the CLI and the GUI route.
    ///
    /// Checks first and returns `updated: false` without downloading when the
    /// model is already current. The HF token comes from the process
    /// environment, matching the CLI. The call does not return until the
    /// download finishes; queue integration is future work, as is any
    /// serialisation between two upgrades of the same model (concurrent
    /// callers both download and the last one wins the row).
    ///
    /// The download is not on the queue, so no snapshot shows it. `rows` is
    /// handed its row while the files are fetched, for a caller that can
    /// draw it; with `None` its progress is shown nowhere.
    pub async fn apply_upgrade(
        &self,
        id: i64,
        rows: Option<RowCallback>,
    ) -> Result<UpgradeOutcome, GuiError> {
        let check = |repo: String, recorded: Option<String>, token| async move {
            gglib_download::cli_exec::check_update(&repo, recorded.as_deref(), token).await
        };
        self.apply_upgrade_with(id, rows, check, gglib_download::cli_exec::update_model)
            .await
    }

    /// [`apply_upgrade`](Self::apply_upgrade) with the two things it asks of
    /// the Hub passed in: `check`, given the repository, the recorded
    /// revision and the token, and `download`, given the request and `rows`.
    async fn apply_upgrade_with<C, CF, D, DF>(
        &self,
        id: i64,
        rows: Option<RowCallback>,
        check: C,
        download: D,
    ) -> Result<UpgradeOutcome, GuiError>
    where
        C: FnOnce(String, Option<String>, Option<String>) -> CF,
        CF: Future<Output = anyhow::Result<UpdateCheckResult>>,
        D: FnOnce(CliUpdateRequest, Option<RowCallback>) -> DF,
        DF: Future<Output = anyhow::Result<CliDownloadResult>> + Send + 'static,
    {
        let mut model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let (repo, quant) = Self::upgrade_source(&model)?;
        let models_dir = gglib_core::paths::resolve_models_dir(None)
            .map_err(|e| GuiError::Internal(format!("Could not resolve models dir: {e}")))?
            .path;

        let check = check(
            repo.clone(),
            model.hf_commit_sha.clone(),
            std::env::var("HF_TOKEN").ok(),
        )
        .await
        .map_err(|e| GuiError::Internal(format!("Update check failed: {e}")))?;
        if !check.has_update {
            return Ok(UpgradeOutcome {
                updated: false,
                latest_sha: check.latest_sha,
                file_path: None,
            });
        }

        // Detached deliberately. The forced re-download deletes the existing
        // file before writing its replacement, so if this ran inline in an
        // Axum request future a client disconnect would drop it mid-transfer
        // and leave the user with no model at all — while the row still
        // pointed at the deleted path. Spawning means the download and the row
        // rewrite always finish as a pair; only the reply is lost.
        let core = self.deps.core.clone();
        let emitter = Arc::clone(&self.deps.emitter);
        let request = CliUpdateRequest {
            model_path: model.file_path.clone(),
            repo_id: repo,
            quantization: quant,
            models_dir,
            token: std::env::var("HF_TOKEN").ok(),
        };

        let download = download(request, rows);
        tokio::spawn(async move {
            let result = download
                .await
                .map_err(|e| GuiError::Internal(format!("Upgrade download failed: {e}")))?;

            model.file_path = result.primary_path.clone();
            model.hf_commit_sha = Some(result.commit_sha.clone());
            model.last_update_check = Some(chrono::Utc::now());
            core.models()
                .update(&model)
                .await
                .map_err(|e| GuiError::Internal(format!("Failed to update model row: {e}")))?;

            // The widest staleness window in the file: this lands minutes
            // after the request that started it, having rewritten `file_path`
            // and `hf_commit_sha`, and by then a second client is likely open.
            emitter.emit(AppEvent::model_updated((&model).into()));

            Ok(UpgradeOutcome {
                updated: true,
                latest_sha: result.commit_sha,
                file_path: Some(result.primary_path.display().to_string()),
            })
        })
        .await
        .map_err(|e| GuiError::Internal(format!("Upgrade task panicked: {e}")))?
    }
}

#[cfg(test)]
#[path = "models_upgrade_tests.rs"]
mod tests;
