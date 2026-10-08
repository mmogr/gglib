//! What a sweep candidate's request carries, read from what a stand-in
//! llama-server received. No model is loaded.
//!
//! A candidate is a statement of exactly what to measure. Its request is what
//! the shared pipeline makes of one naming the candidate's values, and nothing
//! is taken out of it afterwards: a penalty named without a temperature stays
//! in the request wherever the fold resolves none of its own, where a chat's
//! flag of that shape is taken out.

use gglib_core::domain::{DefaultsOrigin, ModelCapabilities};
use gglib_core::request_pipeline::{self, SamplingLayers};
use serde_json::{Value, json};

use super::super::mock_upstream::{MockUpstream, TOOL, read_lines_task};
use super::*;

const MODEL: &str = "mock-model";

fn temperature(value: f32) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(value),
        ..InferenceConfig::default()
    }
}

/// A catalogued model that can call tools, with `defaults` stored as their
/// origin says, tagged `reasoning` when `reasoning` is set.
fn model(defaults: Option<(InferenceConfig, DefaultsOrigin)>, reasoning: bool) -> Model {
    let path = std::path::PathBuf::from("/tmp/model.gguf");
    let new = gglib_core::NewModel::new(MODEL.to_owned(), path, 7.0, chrono::Utc::now());
    let (inference_defaults, defaults_origin) = defaults.unzip();
    Model {
        capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS
            | ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
        tags: reasoning
            .then(|| "reasoning".to_owned())
            .into_iter()
            .collect(),
        inference_defaults,
        defaults_origin,
        ..Model::stored(1, &new)
    }
}

/// The one candidate a sweep of a single value on each axis it names builds.
fn only_candidate(sweep: &SweepSpec) -> InferenceConfig {
    let mut grid = build_candidate_grid(sweep);
    assert_eq!(grid.len(), 1, "one value an axis");
    grid.remove(0)
}

/// The opening request of the mock's task, a turn with tools, run for
/// `candidate` on `model` as the sweep's loop runs it.
async fn opening_request(
    upstream: &MockUpstream,
    model: &Model,
    candidate: &InferenceConfig,
    source: &CandidateSource,
) -> Value {
    let task = read_lines_task();
    let client = BenchmarkDeps::build_http_client().expect("client");
    let target = RunningTarget::local(upstream.port, 1, MODEL.to_owned(), 4096, false);
    let before = upstream.seen().len();
    let result = run_task(
        &client,
        &target,
        model,
        &model_context_for(model),
        &seeded(candidate, &task, source),
        &task,
    )
    .await;
    assert!(result.is_measured(), "{:?}", result.unmeasured);
    upstream.seen().into_iter().nth(before).expect("a request")
}

/// The sampling parameters a request carries, as the server reads them.
fn sampling_of(body: &Value) -> InferenceConfig {
    InferenceConfig::extract_client_sampling(body).0
}

/// On an ordinary model whose stored defaults set a temperature and nothing
/// else, a candidate that names only `min_p`, or only `repeat_penalty`, sends
/// that value beside the temperature: its request is the incumbent's with
/// that one value added.
#[tokio::test]
async fn a_candidate_naming_one_penalty_sends_it_beside_the_models_own_temperature() {
    let upstream = MockUpstream::spawn().await;
    let model = model(Some((temperature(0.55), DefaultsOrigin::User)), false);
    let incumbent = opening_request(
        &upstream,
        &model,
        &InferenceConfig::default(),
        &CandidateSource::Incumbent,
    )
    .await;
    assert_eq!(incumbent["temperature"], json!(0.55_f32), "{incumbent}");

    let sweeps = [
        (
            SweepSpec {
                min_p: vec![0.05],
                ..SweepSpec::default()
            },
            "min_p",
            json!(0.05_f32),
        ),
        (
            SweepSpec {
                repeat_penalty: vec![1.1],
                ..SweepSpec::default()
            },
            "repeat_penalty",
            json!(1.1_f32),
        ),
    ];
    for (sweep, key, value) in sweeps {
        let candidate = only_candidate(&sweep);
        let mut sent =
            opening_request(&upstream, &model, &candidate, &CandidateSource::UserGrid).await;
        assert_eq!(sent[key], value, "{key}: {sent}");
        assert_eq!(sent["temperature"], json!(0.55_f32), "{key}: {sent}");

        sent.as_object_mut().expect("an object").remove(key);
        assert_eq!(sent, incumbent, "{key} is the only difference");
    }
}

/// What the shared pipeline makes of a request with tools that names
/// `candidate`'s values, for `model`, with nothing taken out of it afterwards:
/// what the fold resolved, and beside it each value the candidate named that
/// the fold resolved none for.
fn shaped_by_the_pipeline(candidate: &InferenceConfig, model: &Model) -> InferenceConfig {
    let mut body = Value::Object(candidate.to_openai_json_patch());
    body["messages"] = json!([{"role": "user", "content": "hi"}]);
    body["tools"] = json!([{"type": "function", "function": {"name": TOOL}}]);
    let layers = SamplingLayers {
        trust_client_sampling: true,
        agentic_adjustments: true,
        ..SamplingLayers::default()
    };
    request_pipeline::apply(&mut body, &model_context_for(model), &layers, None)
        .expect("no budget to exceed");
    sampling_of(&body)
}

/// Over every candidate below on every model below, a candidate's request
/// carries the sampling the shared pipeline makes of one naming its values,
/// compared whole: no value a candidate names is taken back out, in any shape.
#[tokio::test]
async fn a_candidates_request_is_what_the_pipeline_makes_of_its_values_and_no_less() {
    let config = |edit: fn(&mut InferenceConfig)| {
        let mut config = InferenceConfig::default();
        edit(&mut config);
        config
    };
    let sweep = |edit: fn(&mut SweepSpec)| {
        let mut sweep = SweepSpec::default();
        edit(&mut sweep);
        sweep
    };
    // The first names nothing, as the incumbent does; the next two are the
    // documented sweeps.
    let sweeps = [
        sweep(|_| {}),
        sweep(|s| s.temperature = vec![0.2, 0.8]),
        sweep(|s| s.dry_multiplier = vec![0.0, 0.8]),
        sweep(|s| s.min_p = vec![0.05]),
        sweep(|s| s.repeat_penalty = vec![1.1]),
        sweep(|s| {
            s.min_p = vec![0.05];
            s.repeat_penalty = vec![1.1];
        }),
        sweep(|s| {
            s.temperature = vec![0.6];
            s.min_p = vec![0.05];
            s.repeat_penalty = vec![1.1];
        }),
        sweep(|s| {
            s.top_p = vec![0.9];
            s.top_k = vec![20];
        }),
        sweep(|s| {
            s.min_p = vec![0.05];
            s.dry_multiplier = vec![0.8];
        }),
        sweep(|s| {
            s.dynatemp_range = vec![0.4];
            s.dynatemp_exponent = vec![1.0];
            s.top_n_sigma = vec![1.0];
        }),
    ];
    let stored = [
        None,
        Some((temperature(0.55), DefaultsOrigin::User)),
        Some((config(|c| c.min_p = Some(0.1)), DefaultsOrigin::User)),
        Some((
            config(|c| {
                c.temperature = Some(0.55);
                c.repeat_penalty = Some(1.05);
            }),
            DefaultsOrigin::User,
        )),
        Some((
            InferenceConfig::reasoning_profile(),
            DefaultsOrigin::AutoDetected,
        )),
        Some((temperature(0.9), DefaultsOrigin::AutoDetected)),
        Some((temperature(0.6), DefaultsOrigin::Measured)),
        Some((
            config(|c| {
                c.temperature = Some(0.8);
                c.top_k = Some(40);
            }),
            DefaultsOrigin::Published,
        )),
    ];

    let upstream = MockUpstream::spawn().await;
    let task = read_lines_task();
    let mut shapes = 0;
    let mut differing = Vec::new();
    for candidate in sweeps.iter().flat_map(build_candidate_grid) {
        for defaults in &stored {
            for reasoning in [false, true] {
                let model = model(defaults.clone(), reasoning);
                let source = CandidateSource::UserGrid;
                let sent = opening_request(&upstream, &model, &candidate, &source).await;
                let expected = shaped_by_the_pipeline(&seeded(&candidate, &task, &source), &model);
                shapes += 1;
                if sampling_of(&sent) != expected {
                    differing.push(format!(
                        "candidate {candidate:?}, model {defaults:?}, reasoning {reasoning}: \
                         sent {:?}, expected {expected:?}",
                        sampling_of(&sent)
                    ));
                }
            }
        }
    }
    assert!(
        differing.is_empty(),
        "{} of {shapes} shapes differ:\n{}",
        differing.len(),
        differing.join("\n")
    );
}
