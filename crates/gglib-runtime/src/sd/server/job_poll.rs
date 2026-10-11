//! Following one submitted job to its end: [`Render`].
//!
//! The render owns its generation turn, and the turn owns the image model's
//! lease. Each new step is reported, previewed, and counted as progress on
//! the turn, which counts it on the lease, so requests queued behind a long
//! render do not give up on a queue that is moving. A render that goes
//! [`IMAGE_STALL`](super::IMAGE_STALL) without a step, or runs past
//! [`IMAGE_JOB_DEADLINE`](super::IMAGE_JOB_DEADLINE), is retired: its server
//! stopped, then its lease released and slot emptied, then its turn ended.
//! So is one whose image model a person asked to stop: the Stop leaves the
//! slot to the render, which lets go of it in that same order.
//!
//! Dropped before its job ends, a render nobody reads any more is stopped
//! ([`leave`]). `sd-server` is asked to cancel the job: a queued one stops
//! there and the turn ends with nothing killed. A generating one cannot be
//! cancelled (409), so the render is retired at once, in that same order,
//! and the next request for the image model loads it again.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use gglib_core::domain::agent::PreviewFrame;
use gglib_core::ports::{GeneratedImage, GenerationTurn, ImageError, ImageStage};
use gglib_core::request_pipeline::{PNG_MIME, image_mime, image_size};
use tokio::time::Instant;
use tracing::{debug, info, warn};

use super::job::{JobTiming, RenderHost, Report};
use super::job_api::{CancelOutcome, JobPreview, JobState, SdJobs};

/// How long a render nobody reads waits for `sd-server`'s answer to its
/// cancel before it is retired as one that is generating.
pub(crate) const CANCEL_WAIT: Duration = Duration::from_secs(5);

/// A submitted job: the server it runs on and its id there.
#[derive(Debug, Clone)]
pub(crate) struct Job {
    pub(crate) base_url: String,
    pub(crate) id: String,
}

/// What one read of the job means for the render.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    /// No step yet.
    Loading,
    /// A step not seen before.
    Step(JobPreview),
    /// Nothing new.
    Same,
    /// Done, with each image's base64.
    Done(Vec<String>),
    /// Over without images, in these words.
    Failed(String),
}

/// The render's clocks and the last step it saw.
#[derive(Debug)]
struct Watch {
    timing: JobTiming,
    submitted: Instant,
    last_step_at: Instant,
    last: Option<(u32, u32)>,
}

impl Watch {
    fn new(timing: JobTiming) -> Self {
        let now = Instant::now();
        Self {
            timing,
            submitted: now,
            last_step_at: now,
            last: None,
        }
    }

    fn see(&mut self, state: JobState) -> Seen {
        match state {
            JobState::Queued | JobState::Generating(None) => Seen::Loading,
            JobState::Generating(Some(preview)) => {
                let at = (preview.pass, preview.step);
                if self.last == Some(at) {
                    return Seen::Same;
                }
                self.last = Some(at);
                self.last_step_at = Instant::now();
                Seen::Step(preview)
            }
            JobState::Completed(images) => Seen::Done(images),
            JobState::Failed(message) => Seen::Failed(message),
            JobState::Cancelled => Seen::Failed("sd-server cancelled the job".to_owned()),
            JobState::Gone => {
                Seen::Failed("sd-server no longer knows the job; it may have restarted".to_owned())
            }
        }
    }

    /// Why the render must be retired now, if it must.
    fn overdue(&self) -> Option<ImageError> {
        let now = Instant::now();
        if now.duration_since(self.submitted) >= self.timing.deadline {
            return Some(ImageError::DeadlineExceeded);
        }
        let quiet = now.duration_since(self.last_step_at);
        (quiet >= self.timing.stall).then_some(ImageError::Stalled { after: quiet })
    }
}

/// One job being followed (see the [module docs](self)).
pub(crate) struct Render {
    turn: Option<GenerationTurn>,
    job: Job,
    jobs: Arc<dyn SdJobs>,
    host: Arc<dyn RenderHost>,
    model_id: u32,
    passes: u32,
    watch: Watch,
}

impl Render {
    /// A render of `passes` images that has just submitted `job`, holding
    /// `turn` until the job ends.
    pub(crate) fn new(
        turn: GenerationTurn,
        job: Job,
        jobs: Arc<dyn SdJobs>,
        host: Arc<dyn RenderHost>,
        model_id: u32,
        timing: JobTiming,
        passes: u32,
    ) -> Self {
        Self {
            turn: Some(turn),
            job,
            jobs,
            host,
            model_id,
            passes,
            watch: Watch::new(timing),
        }
    }

    /// Read the job until it ends, reporting each stage and step; its images.
    pub(crate) async fn follow(
        &mut self,
        report: &Report,
    ) -> Result<Vec<GeneratedImage>, ImageError> {
        loop {
            tokio::time::sleep(self.watch.timing.poll).await;
            match self.jobs.poll(&self.job.base_url, &self.job.id).await {
                Ok(state) => match self.watch.see(state) {
                    Seen::Loading => report.stage(ImageStage::Loading),
                    Seen::Step(preview) => self.step(report, preview),
                    Seen::Same => {}
                    Seen::Done(images) => {
                        self.turn = None;
                        return decode(&images);
                    }
                    Seen::Failed(message) => {
                        self.turn = None;
                        return Err(ImageError::Failed { message });
                    }
                },
                // Transient until the clocks say otherwise.
                Err(e) => debug!(job = %self.job.id, error = %e, "could not read the job"),
            }
            if let Some(error) = ending(&self.watch, self.host.as_ref(), self.model_id) {
                warn!(job = %self.job.id, %error, "retiring the render");
                if let Some(turn) = self.turn.take() {
                    self.host.retire(turn, self.model_id).await;
                }
                return Err(error);
            }
        }
    }

    fn step(&self, report: &Report, preview: JobPreview) {
        if let Some(turn) = &self.turn {
            turn.progress(preview.step, preview.total);
        }
        let stage = ImageStage::Sampling {
            pass: preview.pass,
            step: preview.step,
            total: preview.total,
        };
        let last_pass = preview.pass >= self.passes;
        let last = last_pass && preview.step >= preview.total;
        report.frame(
            stage,
            PreviewFrame::png(preview.step, preview.total, preview.b64),
        );
        if last {
            report.stage(ImageStage::Decoding);
        }
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        let Some(turn) = self.turn.take() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(leave(
            turn,
            self.job.clone(),
            Arc::clone(&self.jobs),
            Arc::clone(&self.host),
            self.model_id,
        ));
    }
}

/// Stop a render whose reader left before its job ended. `turn` is the
/// render's, and with it the image model's lease; this task is the only one
/// that holds it, so the render is stopped once.
///
/// `sd-server` is asked to cancel the job. One it had not started, had
/// finished or no longer knows is over, and the turn ends here with the
/// model left loaded. One that is generating is refused (409) and would run
/// to its end with the GPU held, so the render is retired
/// ([`RenderHost::retire`]): the server killed, then the lease released and
/// the slot emptied, then the turn ended. A cancel that fails, or is not
/// answered within [`CANCEL_WAIT`], is read as a job that is generating.
async fn leave(
    turn: GenerationTurn,
    job: Job,
    jobs: Arc<dyn SdJobs>,
    host: Arc<dyn RenderHost>,
    model_id: u32,
) {
    let cancel = jobs.cancel(&job.base_url, &job.id);
    match tokio::time::timeout(CANCEL_WAIT, cancel).await {
        Ok(Ok(CancelOutcome::Over | CancelOutcome::Gone)) => return,
        Ok(Ok(CancelOutcome::Running)) => {}
        Ok(Err(e)) => debug!(job = %job.id, error = %e, "could not cancel the render"),
        Err(_) => debug!(job = %job.id, "sd-server did not answer the cancel"),
    }
    info!(job = %job.id, "nobody reads the render any more; retiring it");
    host.retire(turn, model_id).await;
}

/// Why the render on `model_id` must be retired now, if it must: a person
/// asked to stop its model, or one of its clocks ran out.
fn ending(watch: &Watch, host: &dyn RenderHost, model_id: u32) -> Option<ImageError> {
    if host.stop_asked(model_id) {
        return Some(ImageError::Stopped);
    }
    watch.overdue()
}

/// Each image's bytes and size, read from the image itself.
fn decode(images: &[String]) -> Result<Vec<GeneratedImage>, ImageError> {
    if images.is_empty() {
        return Err(ImageError::Failed {
            message: "sd-server finished the job with no images".to_owned(),
        });
    }
    images
        .iter()
        .map(|b64| {
            let bytes = BASE64.decode(b64.trim()).map_err(|e| ImageError::Failed {
                message: format!("sd-server returned an image that is not base64: {e}"),
            })?;
            let (width, height) = image_size(&bytes).ok_or_else(|| ImageError::Failed {
                message: "sd-server returned an image whose size cannot be read".to_owned(),
            })?;
            Ok(GeneratedImage {
                mime: image_mime(&bytes).unwrap_or(PNG_MIME),
                bytes,
                width,
                height,
            })
        })
        .collect()
}
