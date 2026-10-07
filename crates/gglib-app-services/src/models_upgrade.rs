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

/// Ask the Hub for `repo`'s latest commit, and compare it with `recorded`.
async fn hub_check(
    repo: String,
    recorded: Option<String>,
    token: Option<String>,
) -> anyhow::Result<UpdateCheckResult> {
    gglib_download::cli_exec::check_update(&repo, recorded.as_deref(), token).await
}

impl ModelOps {
    /// The repository an update check asks about. A model that did not come
    /// from `HuggingFace` has none.
    fn hf_repo(model: &gglib_core::Model) -> Result<String, GuiError> {
        model.hf_repo_id.clone().ok_or_else(|| {
            GuiError::ValidationFailed("Model is not from HuggingFace, cannot update".into())
        })
    }

    /// Preconditions of the upgrade itself: the repository, and the
    /// quantization it downloads again.
    fn upgrade_source(model: &gglib_core::Model) -> Result<(String, String), GuiError> {
        let repo = Self::hf_repo(model)?;
        let quant = model.quantization.clone().ok_or_else(|| {
            GuiError::ValidationFailed("Model has no quantization info stored".into())
        })?;
        Ok((repo, quant))
    }

    /// Whether the model's repository has a commit newer than the one
    /// recorded for it: the comparison behind `gglib model check-updates`,
    /// `gglib model upgrade` and the daemon's upgrade check, distinct from
    /// the shard-level diff on `/{id}/updates`.
    ///
    /// It asks about the repository alone, so a model with no stored
    /// quantization is checked like any other. With no recorded revision
    /// there is nothing to compare: `current_sha` is `None` and `has_update`
    /// is true, which a caller presents as "no baseline recorded", not as a
    /// new release.
    pub async fn check_update(&self, id: i64) -> Result<UpgradeCheck, GuiError> {
        self.check_update_with(id, hub_check).await
    }

    /// [`check_update`](Self::check_update) with what it asks of the Hub
    /// passed in: `check`, given the repository, the recorded revision and
    /// the token.
    async fn check_update_with<C, CF>(&self, id: i64, check: C) -> Result<UpgradeCheck, GuiError>
    where
        C: FnOnce(String, Option<String>, Option<String>) -> CF,
        CF: Future<Output = anyhow::Result<UpdateCheckResult>>,
    {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let repo = Self::hf_repo(&model)?;

        let check = check(repo, model.hf_commit_sha, self.deps.core.hf_token())
            .await
            .map_err(|e| GuiError::Internal(format!("Update check failed: {e}")))?;

        Ok(UpgradeCheck {
            has_update: check.has_update,
            current_sha: check.current_sha,
            latest_sha: check.latest_sha,
        })
    }

    /// [`check_update`](Self::check_update) for a model an upgrade can be
    /// applied to: one with no stored quantization is refused here, as
    /// [`apply_upgrade`](Self::apply_upgrade) would refuse it.
    pub async fn check_upgrade(&self, id: i64) -> Result<UpgradeCheck, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        Self::upgrade_source(&model)?;
        self.check_update(id).await
    }

    /// Re-download the model at the latest `HuggingFace` revision and rewrite
    /// the row — `gglib model upgrade`, shared by the CLI and the GUI route.
    ///
    /// Checks first and returns `updated: false` without downloading when the
    /// model is already current. The HF token is the one the core was built
    /// with, on every surface. The call does not return until the
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
        self.apply_upgrade_with(id, rows, hub_check, gglib_download::cli_exec::update_model)
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
            self.deps.core.hf_token(),
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
            token: self.deps.core.hf_token(),
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
                .map_err(|e| GuiError::from(e).context("Failed to update model row"))?;

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
