//! The job API over HTTP, against the fake `sd-server`'s scripted jobs: what
//! a render is sent, and how each answer reads.

use super::*;
use crate::sd::fake_server::{FakeSdServer, JobScript};

fn ok(body: &str) -> (u16, String) {
    (200, body.to_owned())
}

fn script(polls: Vec<(u16, String)>, cancel: (u16, String)) -> JobScript {
    JobScript {
        submit: (
            202,
            r#"{"id":"job_1","kind":"img_gen","status":"queued"}"#.to_owned(),
        ),
        polls,
        cancel,
    }
}

fn body() -> ImgGenBody {
    ImgGenBody {
        prompt: "a cat".to_owned(),
        width: 1024,
        height: 768,
        seed: -1,
        batch_count: 2,
        output_format: "png",
        preview: "proj",
        preview_interval: 1,
    }
}

async fn served(script: JobScript) -> (FakeSdServer, HttpSdJobs, String) {
    let fake = FakeSdServer::serve_jobs(script).await;
    let base = format!("http://127.0.0.1:{}", fake.port);
    (fake, HttpSdJobs::new(reqwest::Client::new()), base)
}

/// A render is sent as sd-server reads it: PNG out, a `proj` preview at
/// every step, `-1` for a random seed, the image count as `batch_count`.
#[tokio::test]
async fn a_render_is_submitted_with_previews_on_and_its_id_read_back() {
    let (fake, jobs, base) = served(script(vec![ok("{}")], ok("{}"))).await;
    let id = jobs.submit(&base, &body()).await.expect("accepted");
    assert_eq!(id, "job_1");
    let sent: serde_json::Value = serde_json::from_str(&fake.submitted()[0]).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({
            "prompt": "a cat", "width": 1024, "height": 768, "seed": -1,
            "batch_count": 2, "output_format": "png", "preview": "proj",
            "preview_interval": 1
        })
    );
}

/// A refused submission comes back in sd-server's own words.
#[tokio::test]
async fn a_refused_submission_is_sd_servers_words() {
    let refused = JobScript {
        submit: (429, r#"{"error":"job queue is full"}"#.to_owned()),
        ..script(vec![ok("{}")], ok("{}"))
    };
    let (_fake, jobs, base) = served(refused).await;
    assert_eq!(
        jobs.submit(&base, &body()).await.unwrap_err(),
        "job queue is full"
    );
    assert_eq!(
        refusal_words(400, r#"{"error":"invalid json","message":"at 3"}"#),
        "invalid json: at 3"
    );
    assert_eq!(refusal_words(500, ""), "sd-server answered 500");
}

/// Each state reads as what it is: loading before a step, a step with its
/// pass and frame, the images in index order, a failure's message, and a
/// job sd-server no longer knows (404, or 410 past its time to live).
#[tokio::test]
async fn each_job_state_reads_as_what_it_is() {
    let polls = vec![
        ok(r#"{"id":"job_1","status":"queued","queue_position":1}"#),
        ok(r#"{"id":"job_1","status":"generating","preview":null}"#),
        ok(
            r#"{"id":"job_1","status":"generating","preview":{"pass":2,"step":3,"total_steps":4,"b64_json":"AAA"}}"#,
        ),
        ok(
            r#"{"id":"job_1","status":"completed","result":{"output_format":"png","images":[{"index":1,"b64_json":"B"},{"index":0,"b64_json":"A"}]},"error":null}"#,
        ),
        ok(
            r#"{"id":"job_1","status":"failed","result":null,"error":{"code":"generation_failed","message":"generate_image returned no results"}}"#,
        ),
        ok(
            r#"{"id":"job_1","status":"cancelled","error":{"code":"cancelled","message":"job cancelled by client"}}"#,
        ),
        (410, r#"{"error":"job expired"}"#.to_owned()),
        (404, r#"{"error":"job not found"}"#.to_owned()),
    ];
    let (fake, jobs, base) = served(script(polls, ok("{}"))).await;
    let mut states = Vec::new();
    for _ in 0..8 {
        states.push(jobs.poll(&base, "job_1").await.expect("read"));
    }
    assert_eq!(
        states,
        vec![
            JobState::Queued,
            JobState::Generating(None),
            JobState::Generating(Some(JobPreview {
                pass: 2,
                step: 3,
                total: 4,
                b64: "AAA".to_owned()
            })),
            JobState::Completed(vec!["A".to_owned(), "B".to_owned()]),
            JobState::Failed("generate_image returned no results".to_owned()),
            JobState::Cancelled,
            JobState::Gone,
            JobState::Gone,
        ]
    );
    assert_eq!(fake.polled()[0], "/sdcpp/v1/jobs/job_1");
}

/// A cancel of a generating job is a 409: the job runs on.
#[tokio::test]
async fn a_cancel_says_whether_the_job_runs_on() {
    for (status, outcome) in [
        (200, CancelOutcome::Over),
        (409, CancelOutcome::Running),
        (410, CancelOutcome::Gone),
    ] {
        let (fake, jobs, base) = served(script(vec![ok("{}")], (status, "{}".to_owned()))).await;
        assert_eq!(jobs.cancel(&base, "job_1").await.unwrap(), outcome);
        assert_eq!(fake.cancelled(), ["/sdcpp/v1/jobs/job_1/cancel"]);
    }
}
