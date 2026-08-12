//! Applying a tune run's winner as a model's `Measured` defaults, through
//! the gate.
//!
//! This is the write half of the closed loop's first cycle: the gate
//! (`gglib_core::domain::benchmark::tune::apply`) judges the stored run, and
//! only an [`ApplyVerdict::Apply`] licenses touching the model. Every other
//! verdict is returned to the caller as itself — a refusal that names its
//! evidence, never an error.

use anyhow::{Context as _, Result, anyhow};
use gglib_core::domain::DefaultsOrigin;
use gglib_core::domain::benchmark::BenchmarkRunType;
use gglib_core::domain::benchmark::tune::apply::{
    ApplyRecord, ApplyVerdict, evaluate_apply, winning_candidate,
};
use serde::{Deserialize, Serialize};

use super::super::BenchmarkDeps;

/// What an apply attempt returned to its caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyOutcome {
    /// The gate's decision, with its numbers.
    pub verdict: ApplyVerdict,
    /// The model the run tuned.
    pub model_id: i64,
    /// Whether the model was actually written — `verdict.applies()`, echoed
    /// so a wire consumer does not have to know the enum's variants to ask
    /// the one question that matters.
    pub applied: bool,
}

/// Evaluate a completed tune run against the apply gate and, if it passes,
/// store the winner as the model's `Measured` defaults.
///
/// The write is the whole of what changes: `inference_defaults` becomes the
/// winner's sparse overlay (unswept dimensions stay `None` and keep
/// resolving through the chain, exactly as they did during the run), and
/// `defaults_origin` becomes [`DefaultsOrigin::Measured`] — below global
/// settings, exempt from the agentic ceiling, per its own docs. The run row
/// records the [`ApplyRecord`] so the model's provenance stays traceable to
/// the numbers that licensed it.
pub async fn apply_tune_run(deps: &BenchmarkDeps, run_id: i64) -> Result<ApplyOutcome> {
    let run = deps
        .bench_repo
        .get_run(run_id)
        .await
        .context("failed to load run")?
        .ok_or_else(|| anyhow!("run {run_id} not found"))?;
    if run.run_type != BenchmarkRunType::Tune {
        return Err(anyhow!("run {run_id} is not a tune run"));
    }
    let model_id = *run
        .model_ids
        .first()
        .ok_or_else(|| anyhow!("run {run_id} names no model"))?;

    let candidates = deps
        .bench_repo
        .get_tune_results(run_id)
        .await
        .context("failed to load tune results")?;

    let verdict = evaluate_apply(&candidates);
    if !verdict.applies() {
        return Ok(ApplyOutcome {
            verdict,
            model_id,
            applied: false,
        });
    }

    let winner = winning_candidate(&candidates)
        .expect("a verdict of Apply implies a winning candidate exists");

    let mut model = deps
        .model_repo
        .get_by_id(model_id)
        .await
        .with_context(|| format!("model {model_id} not found"))?;
    let prior_defaults = model.inference_defaults.clone();
    let prior_origin = model.defaults_origin;
    model.inference_defaults = Some(winner.config.clone());
    model.defaults_origin = Some(DefaultsOrigin::Measured);
    deps.model_repo
        .update(&model)
        .await
        .context("failed to store the measured defaults")?;

    // Recorded after the model write: a record naming an apply that never
    // happened misleads; an apply missing its record is recoverable from the
    // model's origin alone.
    let record = ApplyRecord {
        verdict: verdict.clone(),
        applied_config: winner.config.clone(),
        prior_defaults: Some(prior_defaults),
        prior_origin: Some(prior_origin),
    };
    if let Ok(json) = serde_json::to_string(&record) {
        deps.bench_repo.mark_run_applied(run_id, &json).await.ok();
    }

    Ok(ApplyOutcome {
        verdict,
        model_id,
        applied: true,
    })
}
