//! The driver against a scripted job and a real admission queue, on a paused
//! clock: what a request is refused for before it queues, how a job's
//! states become stages, and how a stalled, overlong or stopped render is
//! retired. What a render nobody reads any more comes to is in
//! `job_leave_tests.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use gglib_core::contracts::http::images::DrawingAvailability;
use gglib_core::domain::{
    CacheRamHealth, ImageFamily, ModelSamplingDefaults, RuntimeKind, SecondarySlotDecision,
};
use gglib_core::ports::{
    Admission, AdmitObserver, CatalogError, GenerationGate, GenerationTurn, ImageError,
    ImageGenerationPort, ImageProgress, ImageRequest, ImageSize, ImageStage, InMemorySettings,
    ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelSummary, RunningTarget,
};
use gglib_core::settings::Settings;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{JobTiming, RenderHost, SdImageDriver};
use crate::process::admission::{ADMISSION_DEADLINE, AdmissionDecision, Candidate};
use crate::process::{AdmissionQueue, RENDER_STOPPED, Resident};
use crate::sd::server::job_api::{CancelOutcome, ImgGenBody, JobPreview, JobState, SdJobs};

/// The image model's slot: an image model never takes an empty primary.
const SLOT: usize = 1;
const SDXL_ID: u32 = 1;

// ── doubles ──────────────────────────────────────────────────────────────

/// A job whose state is a function of the time since it was submitted.
#[derive(Debug)]
struct Timeline {
    states: Vec<(u64, JobState)>,
    cancel: CancelOutcome,
    /// How long `sd-server` takes to answer a cancel.
    cancel_takes: Duration,
    /// The cancel fails, in these words, instead of answering.
    cancel_fails: Option<String>,
    refuse: Option<String>,
    /// How long `sd-server` takes to answer the submission.
    submit_takes: Duration,
    submitted: Mutex<Option<Instant>>,
    bodies: Mutex<Vec<ImgGenBody>>,
    cancels: AtomicUsize,
}

impl Timeline {
    /// `states`, each from the second given, after submission.
    fn new(states: Vec<(u64, JobState)>) -> Self {
        Self {
            states,
            cancel: CancelOutcome::Over,
            cancel_takes: Duration::ZERO,
            cancel_fails: None,
            refuse: None,
            submit_takes: Duration::ZERO,
            submitted: Mutex::new(None),
            bodies: Mutex::new(Vec::new()),
            cancels: AtomicUsize::new(0),
        }
    }

    fn cancelled_with(mut self, outcome: CancelOutcome) -> Self {
        self.cancel = outcome;
        self
    }
}

#[async_trait]
impl SdJobs for Timeline {
    async fn submit(&self, _base_url: &str, body: &ImgGenBody) -> Result<String, String> {
        if let Some(words) = &self.refuse {
            return Err(words.clone());
        }
        // The job is taken at once; only the answer is slow.
        self.bodies.lock().unwrap().push(body.clone());
        *self.submitted.lock().unwrap() = Some(Instant::now());
        tokio::time::sleep(self.submit_takes).await;
        Ok("job_1".to_owned())
    }

    async fn poll(&self, _base_url: &str, _id: &str) -> Result<JobState, String> {
        let since = self.submitted.lock().unwrap().expect("submitted").elapsed();
        Ok(self
            .states
            .iter()
            .rev()
            .find(|(at, _)| Duration::from_secs(*at) <= since)
            .map(|(_, state)| state.clone())
            .expect("a state from the start"))
    }

    async fn cancel(&self, _base_url: &str, _id: &str) -> Result<CancelOutcome, String> {
        self.cancels.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.cancel_takes).await;
        self.cancel_fails.clone().map_or(Ok(self.cancel), Err)
    }
}

/// The real admission queue behind a stand-in for the manager: admitting
/// installs the image model in its slot, and retiring records what the kill
/// saw before the queue's own retire.
struct Host {
    queue: Arc<AdmissionQueue>,
    admits: AtomicUsize,
    queued_first: Option<usize>,
    log: Arc<Mutex<Vec<String>>>,
    installed: bool,
}

impl Host {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Arc::new(AdmissionQueue::new()),
            admits: AtomicUsize::new(0),
            queued_first: None,
            log: Arc::default(),
            installed: true,
        })
    }

    /// A host with no `sd-server`.
    fn without_runtime() -> Arc<Self> {
        Arc::new(Self {
            installed: false,
            ..Arc::into_inner(Self::new()).unwrap()
        })
    }

    /// The image model's requests in flight, or `None` once its slot is empty.
    fn inflight(&self) -> Option<u32> {
        self.queue.slot(SLOT).map(|r| r.inflight)
    }
}

fn resident(model_id: u32, name: &str) -> Resident {
    Resident {
        model_sampling: ModelSamplingDefaults::default(),
        model_id,
        model_name: name.to_owned(),
        context_size: 0,
        port: 7100,
        projector: None,
        runtime: RuntimeKind::StableDiffusion,
        components: Vec::new(),
        slot_restore_supported: false,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 1,
    }
}

#[async_trait]
impl RenderHost for Host {
    async fn admit(
        &self,
        model: &str,
        observer: Arc<dyn AdmitObserver>,
    ) -> Result<Admission, ModelRuntimeError> {
        self.admits.fetch_add(1, Ordering::SeqCst);
        if let Some(position) = self.queued_first {
            observer.queued(position);
        }
        let id: u32 = model.parse().expect("admitted by id");
        let lease = match self.queue.slot(SLOT) {
            Some(_) => self.queue.lease(SLOT).expect("a lease on the resident"),
            None => self.queue.install(SLOT, resident(id, "sdxl")),
        };
        Ok(Admission {
            target: RunningTarget::local(7100, id, "sdxl".to_owned(), 0, false)
                .with_runtime(RuntimeKind::StableDiffusion),
            lease,
        })
    }

    fn gate(&self) -> Arc<dyn GenerationGate> {
        self.queue.generation_gate()
    }

    async fn runtime_installed(&self) -> bool {
        self.installed
    }

    async fn retire(&self, turn: GenerationTurn, model_id: u32) {
        let queue = Arc::clone(&self.queue);
        let log = Arc::clone(&self.log);
        let kill = async move {
            let seen = queue.slot(SLOT).map(|r| (r.model_id, r.inflight));
            log.lock().unwrap().push(format!("kill, slot {seen:?}"));
        };
        self.queue.retire_render(turn, model_id, kill).await;
        let after = self.queue.slot(SLOT).map(|r| r.model_id);
        self.log
            .lock()
            .unwrap()
            .push(format!("retired, slot {after:?}"));
    }

    fn stop_asked(&self, model_id: u32) -> bool {
        self.queue.render_stop_asked(model_id)
    }

    fn resident_models(&self) -> Vec<u32> {
        self.queue
            .residents()
            .into_iter()
            .map(|(_slot, resident)| resident.model_id)
            .collect()
    }
}

/// Image models and a chat model, by id or name.
#[derive(Debug)]
struct Catalog(Vec<ModelLaunchSpec>);

#[async_trait]
impl ModelCatalogPort for Catalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(self
            .0
            .iter()
            .map(|spec| ModelSummary {
                image_output: spec.image_family.is_some(),
                ..ModelSummary::bare(spec.id, &spec.name)
            })
            .collect())
    }

    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }

    async fn resolve_for_launch(
        &self,
        name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(self
            .0
            .iter()
            .find(|s| s.name == name || s.id.to_string() == name)
            .cloned())
    }
}

fn spec(id: u32, name: &str, family: Option<ImageFamily>) -> ModelLaunchSpec {
    ModelLaunchSpec {
        model_sampling: ModelSamplingDefaults::default(),
        id,
        name: name.to_owned(),
        file_path: format!("/nonexistent/{name}.gguf").into(),
        projector: None,
        image_family: family,
        components: Vec::new(),
        tags: Vec::new(),
        architecture: None,
        quantization: None,
        context_length: None,
        server_defaults: None,
        file_size_bytes: 0,
        kv_elems_per_token: None,
        kv_memory_is_partial: false,
    }
}

/// SDXL, which needs no components; Flux.1 with none of the three it needs;
/// a chat model.
fn models() -> Vec<ModelLaunchSpec> {
    vec![
        spec(SDXL_ID, "sdxl", Some(ImageFamily::Sdxl)),
        spec(2, "flux", Some(ImageFamily::Flux1)),
        spec(3, "qwen", None),
    ]
}

fn driver(catalog: Vec<ModelLaunchSpec>, jobs: &Arc<Timeline>, host: &Arc<Host>) -> SdImageDriver {
    driver_with_default(catalog, None, jobs, host)
}

/// A driver whose settings name `default` as the default image model.
fn driver_with_default(
    catalog: Vec<ModelLaunchSpec>,
    default: Option<i64>,
    jobs: &Arc<Timeline>,
    host: &Arc<Host>,
) -> SdImageDriver {
    let settings = Settings {
        default_image_model_id: default,
        ..Settings::with_defaults()
    };
    SdImageDriver::with_parts(
        Arc::new(Catalog(catalog)),
        Arc::new(InMemorySettings::with(settings)),
        Arc::clone(host) as Arc<dyn RenderHost>,
        Arc::clone(jobs) as Arc<dyn SdJobs>,
        JobTiming::default(),
    )
}

/// A driver that reads its job on `timing`'s clocks.
fn driver_timed(
    catalog: Vec<ModelLaunchSpec>,
    jobs: &Arc<Timeline>,
    host: &Arc<Host>,
    timing: JobTiming,
) -> SdImageDriver {
    SdImageDriver::with_parts(
        Arc::new(Catalog(catalog)),
        Arc::new(InMemorySettings::with(Settings::with_defaults())),
        Arc::clone(host) as Arc<dyn RenderHost>,
        Arc::clone(jobs) as Arc<dyn SdJobs>,
        timing,
    )
}

/// A PNG's signature and `IHDR` at `width` by `height`, base64.
fn png(width: u32, height: u32) -> String {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    BASE64.encode(bytes)
}

fn step(pass: u32, step: u32, total: u32) -> JobState {
    JobState::Generating(Some(JobPreview {
        pass,
        step,
        total,
        b64: format!("frame-{pass}-{step}"),
    }))
}

/// Nothing for 36 s, then four steps 9 s apart, then the image.
fn flux_like() -> Vec<(u64, JobState)> {
    vec![
        (0, JobState::Generating(None)),
        (36, step(1, 1, 4)),
        (45, step(1, 2, 4)),
        (54, step(1, 3, 4)),
        (63, step(1, 4, 4)),
        (72, JobState::Completed(vec![png(1024, 1024)])),
    ]
}

fn sdxl_request() -> ImageRequest {
    ImageRequest {
        model: Some("sdxl".to_owned()),
        ..ImageRequest::new("a lighthouse at dusk")
    }
}

fn drain(rx: &mut mpsc::Receiver<ImageProgress>) -> Vec<ImageProgress> {
    let mut all = Vec::new();
    while let Ok(progress) = rx.try_recv() {
        all.push(progress);
    }
    all
}

fn sampling(pass: u32, step: u32, total: u32) -> ImageStage {
    ImageStage::Sampling { pass, step, total }
}

// ── refused before anything queues ───────────────────────────────────────

/// A size off the family's rule, a chat model, a named model that lacks a
/// file, too many images and an empty prompt are refused before the model is
/// admitted and before any stage is reported.
#[tokio::test(start_paused = true)]
async fn a_request_the_recipe_refuses_never_queues() {
    let jobs = Arc::new(Timeline::new(flux_like()));
    let host = Host::new();
    let driver = driver(models(), &jobs, &host);
    let (tx, mut rx) = mpsc::channel(64);

    let seven = ImageRequest {
        size: Some(ImageSize {
            width: 7,
            height: 7,
        }),
        ..sdxl_request()
    };
    let refused = driver.generate(seven, tx.clone()).await.unwrap_err();
    assert!(
        matches!(
            refused,
            ImageError::InvalidSize {
                width: 7,
                height: 7,
                ..
            }
        ),
        "{refused:?}"
    );
    let chat = ImageRequest {
        model: Some("qwen".to_owned()),
        ..sdxl_request()
    };
    let refused = driver.generate(chat, tx.clone()).await.unwrap_err();
    assert!(
        matches!(&refused, ImageError::NotAnImageModel { model } if model == "qwen"),
        "{refused:?}"
    );
    let five = ImageRequest {
        n: 5,
        ..sdxl_request()
    };
    let refused = driver.generate(five, tx.clone()).await.unwrap_err();
    assert_eq!(refused.code(), Some("invalid_request"), "{refused:?}");
    let blank = ImageRequest {
        prompt: "  ".to_owned(),
        ..sdxl_request()
    };
    let refused = driver.generate(blank, tx.clone()).await.unwrap_err();
    assert_eq!(refused.code(), Some("invalid_request"), "{refused:?}");
    let unknown = ImageRequest {
        model: Some("nope".to_owned()),
        ..sdxl_request()
    };
    let refused = driver.generate(unknown, tx.clone()).await.unwrap_err();
    assert!(
        matches!(
            refused,
            ImageError::Runtime(ModelRuntimeError::ModelNotFound(_))
        ),
        "{refused:?}"
    );
    // Flux lacks its three files: refused by name here, though SDXL, the
    // only complete image model, could have drawn.
    let lacking = ImageRequest {
        model: Some("flux".to_owned()),
        ..sdxl_request()
    };
    let refused = driver.generate(lacking, tx).await.unwrap_err();
    assert!(
        matches!(
            &refused,
            ImageError::Runtime(ModelRuntimeError::ImageModelIncomplete { model, .. })
                if model == "flux"
        ),
        "{refused:?}"
    );

    assert!(drain(&mut rx).is_empty(), "no stage was reported");
    assert_eq!(
        host.admits.load(Ordering::SeqCst),
        0,
        "nothing was admitted"
    );
    assert!(
        jobs.bodies.lock().unwrap().is_empty(),
        "nothing was submitted"
    );
}

/// With no model named, the only complete image model draws; with none, or
/// several, or only one that lacks a file, drawing is unavailable and says
/// why.
#[tokio::test(start_paused = true)]
async fn with_no_model_named_the_only_complete_image_model_draws() {
    let jobs = Arc::new(Timeline::new(flux_like()));
    let host = Host::new();
    let (tx, _rx) = mpsc::channel(64);
    let batch = driver(models(), &jobs, &host)
        .generate(ImageRequest::new("a fox"), tx.clone())
        .await
        .expect("sdxl draws: flux lacks its files");
    assert_eq!(batch.model, "sdxl");

    let reason = |catalog: Vec<ModelLaunchSpec>| {
        let jobs = Arc::clone(&jobs);
        let tx = tx.clone();
        async move {
            match driver(catalog, &jobs, &Host::new())
                .generate(ImageRequest::new("a fox"), tx)
                .await
                .unwrap_err()
            {
                ImageError::Unavailable { reason } => reason,
                other => panic!("expected Unavailable, got {other:?}"),
            }
        }
    };
    let none = reason(vec![spec(3, "qwen", None)]).await;
    assert!(none.contains("no image model"), "{none}");
    let lacking = reason(vec![spec(2, "flux", Some(ImageFamily::Flux1))]).await;
    assert!(lacking.contains("--component vae=<path>"), "{lacking}");
    let several = reason(vec![
        spec(1, "sdxl", Some(ImageFamily::Sdxl)),
        spec(4, "sdxl-turbo", Some(ImageFamily::Sdxl)),
    ])
    .await;
    assert!(several.contains("sdxl, sdxl-turbo"), "{several}");
}

/// With no model named and a default set, the default draws even beside
/// another complete image model; a default that lacks a file is refused
/// with what it lacks; a default that is gone is passed over for the only
/// complete one. A named model wins over the default.
#[tokio::test(start_paused = true)]
async fn with_no_model_named_the_default_image_model_draws_first() {
    let jobs = Arc::new(Timeline::new(flux_like()));
    let (tx, _rx) = mpsc::channel(64);
    let two = || {
        vec![
            spec(SDXL_ID, "sdxl", Some(ImageFamily::Sdxl)),
            spec(4, "sdxl-turbo", Some(ImageFamily::Sdxl)),
            spec(2, "flux", Some(ImageFamily::Flux1)),
        ]
    };
    let draw = |catalog, default, request: ImageRequest| {
        let jobs = Arc::clone(&jobs);
        let tx = tx.clone();
        async move {
            driver_with_default(catalog, default, &jobs, &Host::new())
                .generate(request, tx)
                .await
        }
    };

    let batch = draw(two(), Some(4), ImageRequest::new("a fox")).await;
    assert_eq!(batch.expect("the default draws").model, "sdxl-turbo");

    let named = draw(two(), Some(4), sdxl_request()).await;
    assert_eq!(named.expect("the named model draws").model, "sdxl");

    let lacking = draw(two(), Some(2), ImageRequest::new("a fox")).await;
    match lacking.unwrap_err() {
        ImageError::Unavailable { reason } => {
            assert!(
                reason.contains("flux") && reason.contains("vae"),
                "{reason}"
            );
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }

    let gone = draw(models(), Some(99), ImageRequest::new("a fox")).await;
    assert_eq!(gone.expect("the only complete one draws").model, "sdxl");
    let chat = draw(models(), Some(3), ImageRequest::new("a fox")).await;
    assert_eq!(chat.expect("a chat default is passed over").model, "sdxl");

    let several = draw(two(), None, ImageRequest::new("a fox")).await;
    match several.unwrap_err() {
        ImageError::Unavailable { reason } => {
            assert!(reason.contains("--default-image-model"), "{reason}");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

/// The Draw button's answer, asked without queueing: no runtime is refused
/// before any model is looked at; then core's rule over this driver's
/// catalogue and settings, and its host's residents.
#[tokio::test(start_paused = true)]
async fn the_drawing_answer_is_runtime_first_then_cores_rule_over_this_driver() {
    let jobs = Arc::new(Timeline::new(flux_like()));
    let two = || {
        vec![
            spec(SDXL_ID, "sdxl", Some(ImageFamily::Sdxl)),
            spec(4, "sdxl-turbo", Some(ImageFamily::Sdxl)),
        ]
    };

    let no_runtime = driver(models(), &jobs, &Host::without_runtime())
        .drawing(None)
        .await;
    assert_eq!(
        no_runtime,
        DrawingAvailability::refused(
            "drawing_unavailable",
            ModelRuntimeError::ImageRuntimeNotInstalled.to_string()
        ),
        "refused with no rows"
    );

    // The settings' default is this driver's, and decides between two.
    let defaulted = driver_with_default(two(), Some(4), &jobs, &Host::new())
        .drawing(None)
        .await;
    assert_eq!(defaulted.model.as_deref(), Some("sdxl-turbo"));
    let several = driver(two(), &jobs, &Host::new()).drawing(None).await;
    assert_eq!(several.code.as_deref(), Some("drawing_unavailable"));

    // A chosen model is judged, whatever the default.
    let chosen = driver_with_default(two(), Some(4), &jobs, &Host::new())
        .drawing(Some(SDXL_ID))
        .await;
    assert_eq!(chosen.model_id, Some(SDXL_ID));

    // Resident is what the host's queue holds now: nothing, then SDXL once
    // a render has loaded it.
    let host = Host::new();
    let driver = driver(models(), &jobs, &host);
    let resident = |answer: &DrawingAvailability| {
        answer
            .models
            .iter()
            .map(|row| (row.name.clone(), row.resident))
            .collect::<Vec<_>>()
    };
    let before = driver.drawing(None).await;
    assert_eq!(
        resident(&before),
        [("flux".to_owned(), false), ("sdxl".to_owned(), false)]
    );
    let (tx, _rx) = mpsc::channel(64);
    driver
        .generate(ImageRequest::new("a fox"), tx)
        .await
        .expect("sdxl draws");
    let after = driver.drawing(None).await;
    assert_eq!(
        resident(&after),
        [("flux".to_owned(), false), ("sdxl".to_owned(), true)]
    );
    assert_eq!(
        jobs.bodies.lock().unwrap().len(),
        1,
        "asking submitted nothing; the one render did"
    );
}

// ── a render ─────────────────────────────────────────────────────────────

/// Loading until the first step, each step with its frame, Decoding after
/// the last, the image decoded with its size read from it; the lease held
/// throughout and released at the end.
#[tokio::test(start_paused = true)]
async fn a_render_reports_loading_each_step_and_decoding_then_decodes() {
    let jobs = Arc::new(Timeline::new(flux_like()));
    let host = Host::new();
    let driver = driver(models(), &jobs, &host);
    let (tx, mut rx) = mpsc::channel(64);

    let batch = driver.generate(sdxl_request(), tx).await.expect("drawn");

    let reports = drain(&mut rx);
    let stages: Vec<ImageStage> = reports.iter().map(|p| p.stage.clone()).collect();
    assert_eq!(
        stages,
        vec![
            ImageStage::Loading,
            sampling(1, 1, 4),
            sampling(1, 2, 4),
            sampling(1, 3, 4),
            sampling(1, 4, 4),
            ImageStage::Decoding,
            ImageStage::Finishing,
        ]
    );
    let frame = reports[1]
        .preview
        .as_ref()
        .expect("a step carries its frame");
    assert_eq!((frame.step, frame.total, &*frame.b64), (1, 4, "frame-1-1"));
    assert!(reports[5].preview.is_none(), "decoding has no frame");

    assert_eq!(batch.model, "sdxl");
    assert_eq!(batch.images.len(), 1);
    let image = &batch.images[0];
    assert_eq!(
        (image.width, image.height, image.mime),
        (1024, 1024, "image/png")
    );
    assert!(
        batch.elapsed >= Duration::from_secs(72),
        "{:?}",
        batch.elapsed
    );

    let body = jobs.bodies.lock().unwrap()[0].clone();
    assert_eq!(
        (body.width, body.height, body.seed, body.batch_count),
        (1024, 1024, -1, 1),
        "the family's default square, a random seed"
    );
    assert_eq!((body.preview, body.output_format), ("proj", "png"));
    assert_eq!(host.inflight(), Some(0), "the lease went with the job");
}

/// Two images are two passes: the first pass's last step is not Decoding.
#[tokio::test(start_paused = true)]
async fn decoding_follows_the_last_step_of_the_last_pass() {
    let jobs = Arc::new(Timeline::new(vec![
        (0, JobState::Generating(None)),
        (10, step(1, 2, 2)),
        (20, step(2, 1, 2)),
        (30, step(2, 2, 2)),
        (
            40,
            JobState::Completed(vec![png(1024, 1024), png(1024, 1024)]),
        ),
    ]));
    let host = Host::new();
    let (tx, mut rx) = mpsc::channel(64);
    let request = ImageRequest {
        n: 2,
        seed: Some(7),
        source: None,
        ..sdxl_request()
    };
    let batch = driver(models(), &jobs, &host)
        .generate(request, tx)
        .await
        .expect("drawn");
    let stages: Vec<ImageStage> = drain(&mut rx).into_iter().map(|p| p.stage).collect();
    assert_eq!(
        stages,
        vec![
            ImageStage::Loading,
            sampling(1, 2, 2),
            sampling(2, 1, 2),
            sampling(2, 2, 2),
            ImageStage::Decoding,
            ImageStage::Finishing,
        ]
    );
    assert_eq!(batch.images.len(), 2);
    let body = jobs.bodies.lock().unwrap()[0].clone();
    assert_eq!((body.batch_count, body.seed), (2, 7));
}

/// A place in the admission queue is reported as Queued.
#[tokio::test(start_paused = true)]
async fn a_wait_for_the_model_is_reported_as_queued() {
    let jobs = Arc::new(Timeline::new(flux_like()));
    let host = Arc::new(Host {
        queued_first: Some(2),
        ..Arc::into_inner(Host::new()).unwrap()
    });
    let (tx, mut rx) = mpsc::channel(64);
    driver(models(), &jobs, &host)
        .generate(sdxl_request(), tx)
        .await
        .expect("drawn");
    let stages: Vec<ImageStage> = drain(&mut rx).into_iter().map(|p| p.stage).collect();
    assert_eq!(
        stages[..3],
        [
            ImageStage::Loading,
            ImageStage::Queued {
                position: 2,
                behind: None
            },
            ImageStage::Loading,
        ]
    );
}

/// sd-server's failure is the driver's, in its words, and a job sd-server
/// no longer knows (410) is a failure too; either way the lease goes.
#[tokio::test(start_paused = true)]
async fn a_failed_or_forgotten_job_is_a_failure_in_its_words() {
    for (state, words) in [
        (
            JobState::Failed("generate_image returned no results".to_owned()),
            "generate_image returned no results",
        ),
        (JobState::Gone, "no longer knows the job"),
    ] {
        let jobs = Arc::new(Timeline::new(vec![
            (0, JobState::Generating(None)),
            (5, state),
        ]));
        let host = Host::new();
        let (tx, _rx) = mpsc::channel(64);
        let refused = driver(models(), &jobs, &host)
            .generate(sdxl_request(), tx)
            .await
            .unwrap_err();
        match &refused {
            ImageError::Failed { message } => assert!(message.contains(words), "{message}"),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(refused.code(), Some("image_generation_failed"));
        assert_eq!(host.inflight(), Some(0), "the lease went with the job");
    }
}

/// A refused submission is a failure, and the lease goes with it.
#[tokio::test(start_paused = true)]
async fn a_refused_submission_is_a_failure() {
    let jobs = Arc::new(Timeline {
        refuse: Some("job queue is full".to_owned()),
        ..Timeline::new(flux_like())
    });
    let host = Host::new();
    let (tx, _rx) = mpsc::channel(64);
    let refused = driver(models(), &jobs, &host)
        .generate(sdxl_request(), tx)
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, ImageError::Failed { message } if message == "job queue is full"),
        "{refused:?}"
    );
    assert_eq!(host.inflight(), Some(0));
}

// ── stalls and deadlines ─────────────────────────────────────────────────

/// A job that never steps is retired after three minutes: its server killed
/// while the render still holds the slot and its lease, then the slot
/// emptied, and the request answers Stalled.
#[tokio::test(start_paused = true)]
async fn a_render_with_no_step_for_three_minutes_is_retired() {
    let jobs = Arc::new(Timeline::new(vec![(0, JobState::Generating(None))]));
    let host = Host::new();
    let (tx, _rx) = mpsc::channel(64);
    let refused = driver(models(), &jobs, &host)
        .generate(sdxl_request(), tx)
        .await
        .unwrap_err();
    match refused {
        ImageError::Stalled { after } => assert!(after >= Duration::from_mins(3), "{after:?}"),
        other => panic!("expected Stalled, got {other:?}"),
    }
    assert_eq!(
        *host.log.lock().unwrap(),
        [
            "kill, slot Some((1, 1))".to_owned(),
            "retired, slot None".to_owned()
        ]
    );
}

/// A job that steps every minute and never finishes is retired at thirty.
#[tokio::test(start_paused = true)]
async fn a_render_past_its_deadline_is_retired() {
    let mut states = vec![(0, JobState::Generating(None))];
    states
        .extend((1..=40).map(|minute| (minute * 60, step(1, u32::try_from(minute).unwrap(), 99))));
    let jobs = Arc::new(Timeline::new(states));
    let host = Host::new();
    let (tx, _rx) = mpsc::channel(256);
    let started = Instant::now();
    let refused = driver(models(), &jobs, &host)
        .generate(sdxl_request(), tx)
        .await
        .unwrap_err();
    assert!(
        matches!(refused, ImageError::DeadlineExceeded),
        "{refused:?}"
    );
    let took = started.elapsed();
    assert!(
        took >= Duration::from_mins(30) && took < Duration::from_mins(31),
        "{took:?}"
    );
    assert_eq!(
        host.log.lock().unwrap().last().map(String::as_str),
        Some("retired, slot None")
    );
}

// ── a person's Stop ──────────────────────────────────────────────────────

/// A Stop on the image model while it draws does not empty the slot under
/// the render. The render retires its model at its next read of the job,
/// long before any clock: killed while it still holds the slot and its
/// lease, then the slot emptied and the lease settled with it. A model
/// launched into the slot afterwards keeps its own request: nothing of the
/// old render is left to release there.
#[tokio::test(start_paused = true)]
async fn a_stop_mid_render_retires_it_and_spares_the_slots_next_model() {
    let jobs = Arc::new(Timeline::new(flux_like()));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (tx, _rx) = mpsc::channel(64);
    let task_driver = Arc::clone(&driver);
    let render = tokio::spawn(async move { task_driver.generate(sdxl_request(), tx).await });
    tokio::time::sleep(Duration::from_secs(40)).await;

    assert!(host.queue.ask_render_stop(SDXL_ID), "a render holds it");
    assert_eq!(host.inflight(), Some(1), "the Stop emptied nothing");
    let asked_at = Instant::now();
    let refused = render.await.unwrap().unwrap_err();

    assert!(
        asked_at.elapsed() <= Duration::from_secs(1),
        "at the next read"
    );
    // Its own sentence, with no advice to retry what a person stopped, and
    // the code a failed render has.
    assert!(matches!(refused, ImageError::Stopped), "{refused:?}");
    assert_eq!(refused.to_string(), RENDER_STOPPED);
    assert_eq!(refused.code(), Some("image_generation_failed"));
    assert_eq!(
        *host.log.lock().unwrap(),
        [
            "kill, slot Some((1, 1))".to_owned(),
            "retired, slot None".to_owned()
        ]
    );
    assert!(!host.queue.ask_render_stop(SDXL_ID), "no render is left");

    let newcomer = host.queue.install(SLOT, resident(7, "embedder"));
    drop(driver);
    tokio::time::sleep(Duration::from_mins(31)).await;
    assert_eq!(host.inflight(), Some(1), "the newcomer keeps its request");
    drop(newcomer);
}

/// Each step is progress on the queue: a rival waiting for the image
/// model's slot is still waiting past the admission deadline while the
/// render steps.
#[tokio::test(start_paused = true)]
async fn steps_keep_a_rival_waiting_past_the_admission_deadline() {
    let mut states = vec![(0, JobState::Generating(None))];
    states.extend((0..8).map(|i| (36 + i * 30, step(1, u32::try_from(i + 1).unwrap(), 8))));
    states.push((300, JobState::Completed(vec![png(1024, 1024)])));
    let jobs = Arc::new(Timeline::new(states));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (tx, _rx) = mpsc::channel(64);
    let task_driver = Arc::clone(&driver);
    let render = tokio::spawn(async move { task_driver.generate(sdxl_request(), tx).await });
    tokio::time::sleep(Duration::from_secs(1)).await;

    let rival = host.queue.enqueue(2, "flux");
    let fits = Candidate::image(SecondarySlotDecision::Grant {
        footprint_bytes: 1,
        headroom_bytes: 1 << 40,
    });
    assert_eq!(host.queue.poll(&rival, fits), AdmissionDecision::Wait);
    tokio::time::sleep(Duration::from_secs(250)).await;
    assert!(Duration::from_secs(250) > ADMISSION_DEADLINE);
    assert_eq!(
        host.queue.poll(&rival, fits),
        AdmissionDecision::Wait,
        "the render's steps kept the rival's clock running"
    );
    host.queue.abandon(&rival);
    render.await.unwrap().expect("drawn");
}

#[path = "job_leave_tests.rs"]
mod leave;

#[path = "job_source_tests.rs"]
mod source;
