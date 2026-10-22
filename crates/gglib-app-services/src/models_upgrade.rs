//! Upgrading a model to its repository's latest revision: the check, and
//! the download that follows it.
//!
//! One check for `gglib model upgrade` and the daemon's routes. The CLI
//! downloads in its own process; the daemon queues the download.

use std::future::Future;
use std::sync::Arc;

use gglib_core::download::DownloadError;
use gglib_core::events::AppEvent;
use gglib_core::ports::ModelRegistrarPort;
use gglib_download::cli_exec::{
    CliDownloadResult, CliUpdateRequest, RowCallback, UpdateCheckResult,
};

use crate::error::GuiError;
use crate::models::ModelOps;
use crate::types::{UpgradeCheck, UpgradeOutcome, UpgradeQueued};

/// What an upgrade of a model starts from, once it may go ahead: the model,
/// the repository and quantization it downloads again, and the Hub's answer.
struct Due {
    model: gglib_core::Model,
    repo: String,
    quant: String,
    check: UpdateCheckResult,
}

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
    /// [`apply_upgrade`](Self::apply_upgrade) would refuse it. It does not
    /// ask what is being served: a model the upgrade would refuse for that
    /// is still checked.
    pub async fn check_upgrade(&self, id: i64) -> Result<UpgradeCheck, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        Self::upgrade_source(&model)?;
        self.check_update(id).await
    }

    /// Model `id`, ready to upgrade: one being served is refused before the
    /// Hub is asked anything, then one with no repository or quantization,
    /// and the Hub is asked with the core's token whether it has a newer
    /// commit.
    async fn upgrade_due<C, CF>(&self, id: i64, check: C) -> Result<Due, GuiError>
    where
        C: FnOnce(String, Option<String>, Option<String>) -> CF,
        CF: Future<Output = anyhow::Result<UpdateCheckResult>>,
    {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        self.refuse_if_served(&model).await?;
        let (repo, quant) = Self::upgrade_source(&model)?;
        let check = check(
            repo.clone(),
            model.hf_commit_sha.clone(),
            self.deps.core.hf_token(),
        )
        .await
        .map_err(|e| GuiError::Internal(format!("Update check failed: {e}")))?;
        Ok(Due {
            model,
            repo,
            quant,
            check,
        })
    }

    /// Re-download the model at the latest `HuggingFace` revision, in this
    /// process, and rewrite the row — `gglib model upgrade`.
    ///
    /// The row is rewritten by the registrar's upgrade step
    /// ([`ModelRegistrarPort::upgrade_model`]):
    /// the model keeps its id and every field its owner set, a projector the
    /// download brought is linked when the model has none, and each file's
    /// `model_files` record takes its new OID.
    ///
    /// A model that is being served is refused before the Hub is asked
    /// anything ([`refuse_if_served`](Self::refuse_if_served), the rule
    /// [`remove`](Self::remove) refuses by): the forced download replaces
    /// the file its llama-server has open.
    ///
    /// Checks first and returns `updated: false` without downloading when the
    /// model is already current. The HF token is the one the core was built
    /// with, on every surface. The call does not return until the download
    /// finishes. Nothing serialises two upgrades of the same model this way:
    /// concurrent callers both download and the last one wins the row.
    ///
    /// The download is not on the queue, so no snapshot shows it. `rows` is
    /// handed its row while the files are fetched, for a caller that can
    /// draw it; with `None` its progress is shown nowhere. The daemon's
    /// route queues the download instead
    /// ([`queue_upgrade`](Self::queue_upgrade)).
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
        let Due {
            model,
            repo,
            quant,
            check,
        } = self.upgrade_due(id, check).await?;
        if !check.has_update {
            return Ok(UpgradeOutcome {
                updated: false,
                latest_sha: check.latest_sha,
                file_path: None,
            });
        }
        let models_dir = gglib_core::paths::resolve_models_dir(None)
            .map_err(|e| GuiError::Internal(format!("Could not resolve models dir: {e}")))?
            .path;

        // Detached deliberately. The forced re-download writes each new file
        // beside the old one and moves it into place once complete, so if
        // this ran inline in an Axum request future a client disconnect could
        // drop it after a move and before the row rewrite, leaving new bytes
        // under a row that names the old revision. Spawning means the download
        // and the row rewrite always finish as a pair; only the reply is lost.
        let registrar = self.deps.core.registrar(Arc::clone(&self.deps.gguf_parser));
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

            let upgraded = registrar
                .upgrade_model(id, &result.completed())
                .await
                .map_err(|e| GuiError::from(e).context("Failed to update model row"))?;
            let refusals = upgraded.projector_refusal.iter();
            for refusal in refusals.chain(&upgraded.component_refusals) {
                tracing::warn!(model_id = id, %refusal, "An upgraded model's file was not linked");
            }
            let model = upgraded.model;

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

    /// [`apply_upgrade`](Self::apply_upgrade) on the download queue, for the
    /// daemon's route: it answers once the download is queued and started,
    /// with its ID. The download's row is in the queue snapshot, and
    /// cancelling the download stops it. Once its last file is in, the
    /// manager rewrites the model's row with the registrar's upgrade step
    /// and announces it as `model_updated`.
    ///
    /// The refusals and the check are `apply_upgrade`'s, and a model already
    /// current answers no ID: nothing is queued. The files are fetched at
    /// the commit the check found, which the row then records.
    ///
    /// Each file on disk is kept until its new copy is in place, and is
    /// replaced then, one file at a time: an upgrade cancelled part-way
    /// leaves files of two revisions under a row that names the old one.
    pub async fn queue_upgrade(&self, id: i64) -> Result<UpgradeQueued, GuiError> {
        self.queue_upgrade_with(id, hub_check).await
    }

    /// [`queue_upgrade`](Self::queue_upgrade) with the check passed in, as
    /// [`apply_upgrade_with`](Self::apply_upgrade_with) takes it.
    async fn queue_upgrade_with<C, CF>(&self, id: i64, check: C) -> Result<UpgradeQueued, GuiError>
    where
        C: FnOnce(String, Option<String>, Option<String>) -> CF,
        CF: Future<Output = anyhow::Result<UpdateCheckResult>>,
    {
        let Due {
            repo, quant, check, ..
        } = self.upgrade_due(id, check).await?;
        if !check.has_update {
            return Ok(UpgradeQueued {
                latest_sha: check.latest_sha,
                id: None,
            });
        }

        let commit = check.latest_sha.clone();
        let queued = self
            .deps
            .core
            .downloads()
            .queue_upgrade(repo, quant, id, commit);
        let download = queued.await.map_err(|e| match e {
            DownloadError::AlreadyQueued { id } => GuiError::Conflict(format!(
                "A download of {id} is already waiting or running. Upgrade the model once it \
                 has ended."
            )),
            e => GuiError::Internal(format!("The upgrade could not be queued: {e}")),
        })?;
        Ok(UpgradeQueued {
            latest_sha: check.latest_sha,
            id: Some(download.to_string()),
        })
    }
}

#[cfg(test)]
#[path = "models_upgrade_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "models_upgrade_links_tests.rs"]
mod links_tests;

#[cfg(test)]
#[path = "models_upgrade_queue_tests.rs"]
mod queue_tests;
