//! The proxy arm end to end, against a mock llama-server. No model is loaded.
//!
//! The mock answers every streamed request with a `read_lines` call whose
//! `max_lines` breaks the schema, and a non-streamed one (only the proxy's
//! repair re-issue is non-streamed) with a call that fits it. So an arm that
//! reaches the proxy scores the fixed call, and an arm that does not scores
//! the broken one.

use super::super::mock_upstream::{MockUpstream, NoCatalog, read_lines_task};
use super::*;
use gglib_core::ports::RunningTarget;
use serde_json::Value;

const MODEL: &str = "mock-model";

async fn start_proxy(upstream: &MockUpstream) -> ProxyArm {
    let target = RunningTarget::local(upstream.port, 1, MODEL.to_owned(), 4096, false);
    ProxyArm::start(target, Arc::new(NoCatalog))
        .await
        .expect("the proxy starts")
}

/// Run the mock's task once under `arm`, exactly as the eval's loop does,
/// with a model context that changes nothing.
async fn run_arm(
    arm: EvalArm,
    upstream: &MockUpstream,
    proxy: Option<&ProxyArm>,
    seed: Option<u32>,
) -> TuneTaskResult {
    run_arm_with(arm, upstream, proxy, seed, &ModelContext::passthrough()).await
}

/// [`run_arm`], with the model context the eval hands the pipeline arms.
async fn run_arm_with(
    arm: EvalArm,
    upstream: &MockUpstream,
    proxy: Option<&ProxyArm>,
    seed: Option<u32>,
    context: &ModelContext,
) -> TuneTaskResult {
    let task = read_lines_task();
    let client = BenchmarkDeps::build_agentic_http_client().expect("client");
    let upstream_url = upstream.base_url();
    let url = arm_base_url(arm, &upstream_url, proxy).to_owned();
    run_task_with_llm(
        |usage| build_arm_llm(&client, &url, MODEL, arm, context, &task, seed, usage),
        &task,
    )
    .await
}

/// The first chat body the upstream saw after `before` bodies.
fn first_body_since(upstream: &MockUpstream, before: usize) -> Value {
    upstream.seen().into_iter().nth(before).expect("a request")
}

/// **The point of the arm.** Only the proxy arm's call is repaired: the
/// proxy judges the broken call, re-issues it, and hands the agent the fixed
/// one. Every other arm scores the call the model made.
///
/// The proxy is running while the other arms run, as it is in the eval, so
/// an arm routed to it by mistake shows in the proxy's request count.
#[tokio::test]
async fn a_violating_first_answer_is_repaired_through_the_proxy_arm_and_not_the_others() {
    let upstream = MockUpstream::spawn().await;
    let proxy = start_proxy(&upstream).await;
    for arm in [EvalArm::Raw, EvalArm::Gglib, EvalArm::RawAuto] {
        let result = run_arm(arm, &upstream, Some(&proxy), Some(1)).await;
        assert!(result.is_measured(), "{arm}: {:?}", result.unmeasured);
        assert!(
            result.tool_match_score < 1.0,
            "{arm} scored the broken call as fixed"
        );
    }

    let result = run_arm(EvalArm::Proxy, &upstream, Some(&proxy), Some(1)).await;
    assert!(result.is_measured(), "{:?}", result.unmeasured);
    assert!(
        (result.tool_match_score - 1.0).abs() < f64::EPSILON,
        "the proxy arm scored {}, not the repaired call",
        result.tool_match_score
    );
    let (defects, settings) = proxy.finish().await;
    assert!(settings.tool_call_repair);
    // The proxy arm's two turns (the call, then the answer after the tool
    // result), and nothing from the arms before it.
    assert_eq!(
        (
            defects.requests,
            defects.repairs_attempted,
            defects.repairs_succeeded
        ),
        (2, 1, 1),
        "{defects:?}"
    );
    let reissue = upstream
        .seen()
        .into_iter()
        .find(|body| body["stream"] == Value::Bool(false))
        .expect("the proxy re-issued the call");
    assert_eq!(reissue["tool_choice"], "required");
}

/// Under `"auto"` the proxy judges every call whose schema it can judge, so
/// the proxy pair opens with it,
/// and every other arm keeps the `"required"` the ADR 0004 readings were taken
/// under.
#[tokio::test]
async fn the_proxy_arm_opens_with_auto_and_the_others_with_required() {
    let upstream = MockUpstream::spawn().await;
    let proxy = start_proxy(&upstream).await;
    for (arm, expected) in [
        (EvalArm::Raw, "required"),
        (EvalArm::Gglib, "required"),
        (EvalArm::RawAuto, "auto"),
        (EvalArm::Proxy, "auto"),
    ] {
        let before = upstream.seen().len();
        run_arm(arm, &upstream, Some(&proxy), Some(1)).await;
        let opening = first_body_since(&upstream, before);
        assert_eq!(opening["tool_choice"], expected, "{arm}: {opening}");
    }
}

/// The baseline is the raw arm with one difference, or the pair's delta
/// measures two things.
#[tokio::test]
async fn the_raw_auto_arm_differs_from_raw_only_in_tool_choice() {
    let upstream = MockUpstream::spawn().await;
    run_arm(EvalArm::Raw, &upstream, None, Some(3)).await;
    let before = upstream.seen().len();
    run_arm(EvalArm::RawAuto, &upstream, None, Some(3)).await;
    let mut raw = first_body_since(&upstream, 0);
    let mut raw_auto = first_body_since(&upstream, before);
    for body in [&mut raw, &mut raw_auto] {
        body.as_object_mut().expect("object").remove("tool_choice");
    }
    assert_eq!(raw, raw_auto);
}

/// The proxy strips a client's seed unless client sampling is trusted, and
/// an unseeded arm cannot be compared run for run with a seeded one.
#[tokio::test]
async fn the_proxy_arm_carries_its_seed_upstream() {
    let upstream = MockUpstream::spawn().await;
    let proxy = start_proxy(&upstream).await;
    run_arm(EvalArm::Proxy, &upstream, Some(&proxy), Some(4242)).await;
    let opening = first_body_since(&upstream, 0);
    assert_eq!(opening["seed"], 4242, "{opening}");
}

/// The proxy applies the request pipeline itself, so the proxy arm's client
/// sends a bare body. Were the client to apply it too, the model's sampling
/// would reach llama-server through the proxy on the client's say, and the
/// pipeline would run twice.
///
/// The context names a temperature no default carries. The gglib arm, which
/// applies the pipeline on the client, shows that the context does put it on
/// the wire; the proxy arm, given the same context, must not.
#[tokio::test]
async fn the_proxy_arm_leaves_the_request_pipeline_to_the_proxy() {
    let context = ModelContext {
        inference_defaults: Some(gglib_core::domain::InferenceConfig {
            temperature: Some(0.123),
            ..Default::default()
        }),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let upstream = MockUpstream::spawn().await;
    let proxy = start_proxy(&upstream).await;

    run_arm_with(EvalArm::Gglib, &upstream, None, Some(1), &context).await;
    let through_the_client = first_body_since(&upstream, 0);
    let temperature = through_the_client["temperature"].as_f64();
    assert!(
        temperature.is_some_and(|t| (t - 0.123).abs() < 1e-6),
        "the context's temperature reaches the wire when the client applies the pipeline: \
         {through_the_client}"
    );

    let before = upstream.seen().len();
    run_arm_with(EvalArm::Proxy, &upstream, Some(&proxy), Some(1), &context).await;
    let through_the_proxy = first_body_since(&upstream, before);
    let temperature = through_the_proxy["temperature"].as_f64();
    assert!(
        !temperature.is_some_and(|t| (t - 0.123).abs() < 1e-6),
        "the proxy arm's client applied the pipeline: {through_the_proxy}"
    );
}

/// Off unless asked for; when asked for, both run on the primary seeds,
/// straight after the two real arms.
#[test]
fn the_proxy_pair_is_planned_only_when_asked_for_and_after_the_real_arms() {
    let config = |include_proxy: bool| AgenticEvalConfig {
        include_proxy,
        ..serde_json::from_str(r#"{"model_id": 1, "task_suite": {"source": "default"}}"#)
            .expect("config")
    };
    let arms = |plans: Vec<ArmPlan>| plans.into_iter().map(|p| p.arm).collect::<Vec<_>>();
    assert!(!arms(plan_arms(&config(false))).contains(&EvalArm::Proxy));

    let plans = plan_arms(&config(true));
    let primary = plans[0].seeds.clone();
    assert_eq!(
        arms(plan_arms(&config(true)))[..4],
        [
            EvalArm::Raw,
            EvalArm::Gglib,
            EvalArm::RawAuto,
            EvalArm::Proxy
        ]
    );
    assert_eq!(plans[2].seeds, primary);
    assert_eq!(plans[3].seeds, primary);
}
