//! [`SdImageDriver`]: the one way gglib draws, an async job on `sd-server`.
//!
//! A request is planned before anything queues (`job_plan.rs`), then the
//! image model is admitted, which launches `sd-server` if it is not resident,
//! and the render takes its turn on the generation gate with that lease, so
//! no chat generates while it draws and the model cannot be swapped out. The
//! job is submitted with `preview: proj`, because `sd-server` reports steps
//! only while a preview mode is on, and read about once a second
//! (`job_poll.rs`): nothing reports a step until the first one finishes,
//! about 40 s into a Flux render, so the stage is Loading until then.
//!
//! `sd-server` cannot interrupt a generating job. A request dropped part way
//! (a client that left) cancels the job and keeps the turn, and with it the
//! lease, until the job ends, under the same stall and deadline rules. That
//! holds from the moment the job is submitted: the submission runs in its
//! own task, so a request dropped during it still ends in a render that is
//! cancelled and followed.
//!
//! A person's Stop on the image model never empties its slot under a render.
//! It asks, and the render, dropped or not, retires its model at its next
//! read of the job: the server stopped, the lease settled with the slot it
//! was counted in, the turn ended.

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::domain::agent::PreviewFrame;
use gglib_core::ports::{
    Admission, AdmitObserver, GateWait, GateWaitObserver, GenerationGate, GenerationTurn,
    ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ImageStage,
    LaunchOverrides, ModelCatalogPort, ModelRuntimeError, SettingsRepository,
};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use super::job_api::{HttpSdJobs, SdJobs};
use super::job_plan::{Plan, drawing_model, plan};
use super::job_poll::{Job, Render};
use crate::process::ProcessManager;

/// How often a job's state is read.
pub const POLL: Duration = Duration::from_secs(1);

/// How long a render may go without a new step, counted from submission
/// (the load before the first step included), before its model is stopped.
pub const IMAGE_STALL: Duration = Duration::from_mins(3);

/// How long a render may take from submission, however it steps; core's,
/// so the drawing tool's deadline is the same number.
pub use gglib_core::ports::IMAGE_JOB_DEADLINE;

/// The three clocks a render runs under; the constants above, or a test's.
#[derive(Debug, Clone, Copy)]
pub(crate) struct JobTiming {
    pub(crate) poll: Duration,
    pub(crate) stall: Duration,
    pub(crate) deadline: Duration,
}

impl Default for JobTiming {
    fn default() -> Self {
        Self {
            poll: POLL,
            stall: IMAGE_STALL,
            deadline: IMAGE_JOB_DEADLINE,
        }
    }
}

/// Where the driver gets its model, its turn, and how it ends a render that
/// stopped moving: the [`ProcessManager`], or a test's stand-in.
#[async_trait]
pub(crate) trait RenderHost: Send + Sync {
    /// Admit `model`, telling `observer` its place while it waits.
    async fn admit(
        &self,
        model: &str,
        observer: Arc<dyn AdmitObserver>,
    ) -> Result<Admission, ModelRuntimeError>;

    /// The generation gate.
    fn gate(&self) -> Arc<dyn GenerationGate>;

    /// Whether `sd-server` is installed where a launch looks for it.
    async fn runtime_installed(&self) -> bool;

    /// Stop `model_id`'s server, then release the render's lease and empty
    /// its slot, then end `turn`: [`ProcessManager::retire_render`].
    async fn retire(&self, turn: GenerationTurn, model_id: u32);

    /// Whether a person asked to stop `model_id` while this render draws
    /// with it: [`ProcessManager::render_stop_asked`].
    fn stop_asked(&self, model_id: u32) -> bool;
}

#[async_trait]
impl RenderHost for ProcessManager {
    async fn admit(
        &self,
        model: &str,
        observer: Arc<dyn AdmitObserver>,
    ) -> Result<Admission, ModelRuntimeError> {
        self.admit_observed(
            model,
            None,
            None,
            LaunchOverrides::default(),
            Some(observer),
        )
        .await
    }

    fn gate(&self) -> Arc<dyn GenerationGate> {
        self.generation_gate()
    }

    async fn runtime_installed(&self) -> bool {
        self.image_runtime_installed().await
    }

    async fn retire(&self, turn: GenerationTurn, model_id: u32) {
        self.retire_render(turn, model_id).await;
    }

    fn stop_asked(&self, model_id: u32) -> bool {
        self.render_stop_asked(model_id)
    }
}

/// Draws through `sd-server` (see the [module docs](self)).
pub struct SdImageDriver {
    catalog: Arc<dyn ModelCatalogPort>,
    settings: Arc<dyn SettingsRepository>,
    host: Arc<dyn RenderHost>,
    jobs: Arc<dyn SdJobs>,
    timing: JobTiming,
}

impl fmt::Debug for SdImageDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SdImageDriver").finish_non_exhaustive()
    }
}

impl SdImageDriver {
    /// A driver over the daemon's one `manager`, resolving models through
    /// `catalog` and the default image model through `settings`.
    #[must_use]
    pub fn new(
        catalog: Arc<dyn ModelCatalogPort>,
        settings: Arc<dyn SettingsRepository>,
        manager: Arc<ProcessManager>,
    ) -> Self {
        Self::with_parts(
            catalog,
            settings,
            manager,
            Arc::new(HttpSdJobs::new(gglib_proxy::loopback::client())),
            JobTiming::default(),
        )
    }

    /// A driver from its parts, for tests.
    pub(crate) fn with_parts(
        catalog: Arc<dyn ModelCatalogPort>,
        settings: Arc<dyn SettingsRepository>,
        host: Arc<dyn RenderHost>,
        jobs: Arc<dyn SdJobs>,
        timing: JobTiming,
    ) -> Self {
        Self {
            catalog,
            settings,
            host,
            jobs,
            timing,
        }
    }

    /// Submit the job and hand back the render that follows it, in a task
    /// of its own that owns `turn`: a request dropped while the job is being
    /// submitted does not drop the submission with it. `sd-server` may have
    /// taken the job by then, so the task finishes the submission and drops
    /// the render it would have handed back, which cancels the job and keeps
    /// the turn, and with it the lease, until the job is over, as a render
    /// dropped any later does.
    async fn submit(
        &self,
        turn: GenerationTurn,
        base_url: String,
        plan: &Plan,
    ) -> Result<Render, ImageError> {
        let (jobs, host) = (Arc::clone(&self.jobs), Arc::clone(&self.host));
        let (body, model_id, timing) = (plan.body.clone(), plan.model_id, self.timing);
        let passes = u32::from(plan.n);
        let (made, submitted) = oneshot::channel();
        tokio::spawn(async move {
            let render = match jobs.submit(&base_url, &body).await {
                Ok(id) => {
                    let job = Job { base_url, id };
                    Ok(Render::new(turn, job, jobs, host, model_id, timing, passes))
                }
                Err(message) => {
                    drop(turn);
                    Err(ImageError::Failed { message })
                }
            };
            // Nobody to hand it to: the render is dropped here, in the task.
            drop(made.send(render));
        });
        submitted.await.unwrap_or_else(|_| {
            Err(ImageError::Failed {
                message: "the job's submission was cut short".to_owned(),
            })
        })
    }

    /// The settings' default image model, read when a request names none.
    /// A store that cannot be read counts as no default: the only complete
    /// image model may still draw.
    async fn default_model(&self, request: &ImageRequest) -> Option<i64> {
        if request.model.is_some() {
            return None;
        }
        match self.settings.load().await {
            Ok(settings) => settings.default_image_model_id,
            Err(e) => {
                tracing::warn!("could not read the default image model: {e}");
                None
            }
        }
    }
}

#[async_trait]
impl ImageGenerationPort for SdImageDriver {
    async fn generate(
        &self,
        request: ImageRequest,
        progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        let started = Instant::now();
        let default_model = self.default_model(&request).await;
        let plan = plan(self.catalog.as_ref(), default_model, &request).await?;
        let report = Report::new(progress);
        report.stage(ImageStage::Loading);

        let observer: Arc<dyn AdmitObserver> = Arc::new(report.clone());
        let admission = self
            .host
            .admit(&plan.model_id.to_string(), observer)
            .await
            .map_err(ImageError::Runtime)?;
        let base_url = admission.target.base_url.clone();
        report.stage(ImageStage::Loading);

        let waits: Arc<dyn GateWaitObserver> = Arc::new(report.clone());
        let turn = self
            .host
            .gate()
            .render_turn(admission.lease, Some(waits))
            .await
            .map_err(ImageError::Gate)?;
        report.stage(ImageStage::Loading);

        let mut render = self.submit(turn, base_url, &plan).await?;
        let images = render.follow(&report).await?;
        report.stage(ImageStage::Finishing);
        Ok(ImageBatch {
            model: plan.model_name,
            images,
            elapsed: started.elapsed(),
        })
    }

    /// No runtime first, as a launch would find it, then the model rule a
    /// request that names none is planned by.
    async fn drawing_model(&self) -> Result<String, ImageError> {
        if !self.host.runtime_installed().await {
            return Err(ImageError::Unavailable {
                reason: ModelRuntimeError::ImageRuntimeNotInstalled.to_string(),
            });
        }
        let default_model = self.default_model(&ImageRequest::new("")).await;
        drawing_model(self.catalog.as_ref(), default_model)
            .await
            .map(|spec| spec.name)
    }
}

/// Sends a render's reports, never waiting, and never the same stage twice
/// running; a stage with a frame is always sent.
#[derive(Debug, Clone)]
pub(crate) struct Report {
    tx: mpsc::Sender<ImageProgress>,
    last: Arc<Mutex<Option<ImageStage>>>,
}

impl Report {
    fn new(tx: mpsc::Sender<ImageProgress>) -> Self {
        Self {
            tx,
            last: Arc::new(Mutex::new(None)),
        }
    }

    /// Report `stage`, unless it was the last one reported.
    pub(crate) fn stage(&self, stage: ImageStage) {
        self.send(stage, None);
    }

    /// Report `stage` with the frame it made.
    pub(crate) fn frame(&self, stage: ImageStage, frame: PreviewFrame) {
        self.send(stage, Some(frame));
    }

    fn send(&self, stage: ImageStage, preview: Option<PreviewFrame>) {
        {
            let mut last = self
                .last
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if preview.is_none() && last.as_ref() == Some(&stage) {
                return;
            }
            *last = Some(stage.clone());
        }
        // A reader that is behind loses this report, never slows the render.
        let _ = self.tx.try_send(ImageProgress { stage, preview });
    }
}

impl AdmitObserver for Report {
    fn queued(&self, position: usize) {
        self.stage(ImageStage::Queued {
            position: u32::try_from(position).unwrap_or(u32::MAX),
            behind: None,
        });
    }
}

impl GateWaitObserver for Report {
    fn waiting(&self, wait: GateWait) {
        self.stage(ImageStage::Queued {
            position: u32::try_from(wait.position).unwrap_or(u32::MAX),
            behind: Some("an image render".to_owned()),
        });
    }
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
