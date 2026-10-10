//! `sd-server`'s async job API, as the image driver uses it: submit a render,
//! read its state, ask it to stop.
//!
//! stable-diffusion.cpp 228c707 (`examples/server`): `POST
//! /sdcpp/v1/img_gen` answers 202 with the job's `id` (400 for a request it
//! cannot read, 429 when its queue is full, each with
//! `{"error": <words>, "message"?: <more>}`); `GET /sdcpp/v1/jobs/{id}`
//! answers the job's `status`, `queued`, `generating`, `completed`, `failed`
//! or `cancelled`, with `preview {pass, step, total_steps, b64_json}` only
//! while generating and only once the first step has finished,
//! `result.images[{index, b64_json}]` once completed and `error {code,
//! message}` once failed, and 404, or 410 once the job outlived its time to
//! live; `POST /sdcpp/v1/jobs/{id}/cancel` answers 200 for a queued or
//! finished job and 409 for a generating one, which it cannot interrupt.
//!
//! [`SdJobs`] is the seam the driver's tests replace with a script; the HTTP
//! client here is the only implementation outside tests.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// How long one call to the job API may take. Every answer is small but the
/// completed job's, which carries the images (about 2 MB of base64 each at
/// 1024x1024), and all of it is local.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// One render, as `POST /sdcpp/v1/img_gen` reads it. Steps, guidance and
/// sampler come from the server's launch flags (the family's recipe).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImgGenBody {
    pub(crate) prompt: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// `-1` for a random seed.
    pub(crate) seed: i64,
    pub(crate) batch_count: u8,
    /// Always `png`.
    pub(crate) output_format: &'static str,
    /// Always `proj`: steps are reported only while a preview mode is on.
    pub(crate) preview: &'static str,
    /// Every step.
    pub(crate) preview_interval: u32,
}

/// The step a generating job last previewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobPreview {
    /// The sampling pass, 1-based: one per image.
    pub(crate) pass: u32,
    /// The step finished.
    pub(crate) step: u32,
    /// The steps the pass takes.
    pub(crate) total: u32,
    /// The preview frame, base64 PNG.
    pub(crate) b64: String,
}

/// What a job's status read says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JobState {
    /// Waiting in sd-server's own queue.
    Queued,
    /// Running; `preview` once the first step has finished.
    Generating(Option<JobPreview>),
    /// Done: each image's base64, in index order.
    Completed(Vec<String>),
    /// Failed, in sd-server's words.
    Failed(String),
    /// Cancelled before it ran.
    Cancelled,
    /// sd-server no longer knows the job: 404, or 410 past its time to live.
    Gone,
}

/// What a cancel achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelOutcome {
    /// The job is over: it was queued and is now cancelled, or had finished.
    Over,
    /// The job is generating and runs on to its end (409).
    Running,
    /// sd-server no longer knows the job.
    Gone,
}

/// The job API (see the [module docs](self)). Errors are the transport's or
/// sd-server's own words.
#[async_trait]
pub(crate) trait SdJobs: Send + Sync + std::fmt::Debug {
    /// Submit a render to the server at `base_url`; its job id.
    async fn submit(&self, base_url: &str, body: &ImgGenBody) -> Result<String, String>;
    /// Read a job's state.
    async fn poll(&self, base_url: &str, id: &str) -> Result<JobState, String>;
    /// Ask the server to stop a job.
    async fn cancel(&self, base_url: &str, id: &str) -> Result<CancelOutcome, String>;
}

/// [`SdJobs`] over HTTP.
#[derive(Debug, Clone)]
pub(crate) struct HttpSdJobs {
    client: reqwest::Client,
}

impl HttpSdJobs {
    /// A job client over `client`.
    pub(crate) const fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
struct Submitted {
    id: String,
}

#[derive(Deserialize)]
struct Job {
    status: String,
    #[serde(default)]
    preview: Option<WirePreview>,
    #[serde(default)]
    result: Option<WireResult>,
    #[serde(default)]
    error: Option<WireError>,
}

#[derive(Deserialize)]
struct WirePreview {
    #[serde(default)]
    pass: u32,
    step: u32,
    total_steps: u32,
    b64_json: String,
}

#[derive(Deserialize)]
struct WireResult {
    #[serde(default)]
    images: Vec<WireImage>,
}

#[derive(Deserialize)]
struct WireImage {
    #[serde(default)]
    index: usize,
    b64_json: String,
}

#[derive(Deserialize)]
struct WireError {
    #[serde(default)]
    message: String,
}

impl Job {
    fn into_state(self) -> JobState {
        match self.status.as_str() {
            "queued" => JobState::Queued,
            "generating" => JobState::Generating(self.preview.map(|p| JobPreview {
                pass: p.pass.max(1),
                step: p.step,
                total: p.total_steps,
                b64: p.b64_json,
            })),
            "completed" => {
                let mut images = self.result.map(|r| r.images).unwrap_or_default();
                images.sort_by_key(|i| i.index);
                JobState::Completed(images.into_iter().map(|i| i.b64_json).collect())
            }
            "cancelled" => JobState::Cancelled,
            other => JobState::Failed(
                self.error
                    .map(|e| e.message)
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| format!("the job ended {other}")),
            ),
        }
    }
}

/// sd-server's words from a refusal body: `error`, then `message` after it
/// when there is one, else the body itself.
pub(crate) fn refusal_words(status: u16, body: &str) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let field = |key: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    match (field("error"), field("message")) {
        (Some(error), Some(message)) => format!("{error}: {message}"),
        (Some(error), None) => error,
        _ if body.trim().is_empty() => format!("sd-server answered {status}"),
        _ => format!("sd-server answered {status}: {}", body.trim()),
    }
}

#[async_trait]
impl SdJobs for HttpSdJobs {
    async fn submit(&self, base_url: &str, body: &ImgGenBody) -> Result<String, String> {
        let response = self
            .client
            .post(format!("{base_url}/sdcpp/v1/img_gen"))
            .timeout(CALL_TIMEOUT)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("could not reach sd-server: {e}"))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(refusal_words(status.as_u16(), &text));
        }
        serde_json::from_str::<Submitted>(&text)
            .map(|s| s.id)
            .map_err(|e| format!("sd-server's answer had no job id: {e}"))
    }

    async fn poll(&self, base_url: &str, id: &str) -> Result<JobState, String> {
        let response = self
            .client
            .get(format!("{base_url}/sdcpp/v1/jobs/{id}"))
            .timeout(CALL_TIMEOUT)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        if matches!(status, 404 | 410) {
            return Ok(JobState::Gone);
        }
        let text = response.text().await.map_err(|e| e.to_string())?;
        if !(200..300).contains(&status) {
            return Err(refusal_words(status, &text));
        }
        serde_json::from_str::<Job>(&text)
            .map(Job::into_state)
            .map_err(|e| format!("an unreadable job state: {e}"))
    }

    async fn cancel(&self, base_url: &str, id: &str) -> Result<CancelOutcome, String> {
        let response = self
            .client
            .post(format!("{base_url}/sdcpp/v1/jobs/{id}/cancel"))
            .timeout(CALL_TIMEOUT)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        match response.status().as_u16() {
            409 => Ok(CancelOutcome::Running),
            404 | 410 => Ok(CancelOutcome::Gone),
            s if (200..300).contains(&s) => Ok(CancelOutcome::Over),
            s => Err(refusal_words(s, &response.text().await.unwrap_or_default())),
        }
    }
}

#[cfg(test)]
#[path = "job_api_tests.rs"]
mod tests;
