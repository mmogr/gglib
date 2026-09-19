//! Tests for [`super`]: the benchmark repository against a real database.

use gglib_core::domain::benchmark::agentic::{ArmDelta, ArmScores};
use gglib_core::domain::benchmark::tune::result::GeneratedOutput;

use crate::setup::setup_test_database;

use super::*;
use gglib_core::domain::benchmark::{DEFAULT_SEEDS, replicate_seeds};

fn arm(composite: f64, loop_avoidance: Option<f64>) -> ArmScores {
    ArmScores {
        tool_accuracy: 0.722,
        loop_avoidance,
        loop_eligible: usize::from(loop_avoidance.is_some()),
        task_completion: 0.667,
        composite,
        tg_tps: Some(205.3),
        total_completion_tokens: Some(226_768),
        total_wall_ms: 1_104_543,
        measured_wall_ms: 1_104_543,
        mean_time_to_first_tool_call_ms: Some(2_100.0),
        median_time_to_first_tool_call_ms: Some(2_100.0),
        generated: GeneratedOutput::default(),
        seeds: 3,
        runs: 9,
        unmeasured_runs: 0,
        transport_retries: 0,
    }
}

fn report() -> AgenticEvalReport {
    let raw = arm(0.802, None);
    let gglib = arm(0.802, Some(1.0));
    AgenticEvalReport {
        model_name: "qwen2.5-0.5b-instruct".to_owned(),
        quantization: Some("Q8_0".to_owned()),
        param_count_b: 0.5,
        ctx_size: 131_072,
        delta: ArmDelta {
            tool_accuracy: Some(0.0),
            loop_avoidance: None,
            task_completion: Some(0.0),
            composite: Some(0.0),
            wall_time_speedup: Some(229.83),
            completion_token_ratio: Some(4_627.92),
            withheld: None,
        },
        raw,
        gglib,
        tasks: vec![],
        seeds: DEFAULT_SEEDS.to_vec(),
        // The control runs fewer seeds than the arms it validates, and a
        // fixture that gave it the same count would not round-trip the
        // per-arm sample size this table is supposed to preserve.
        control: Some(ArmScores {
            seeds: 1,
            runs: 3,
            ..arm(0.60, Some(1.0))
        }),
        raw_replicate: Some(arm(0.780, None)),
        replicate_seeds: replicate_seeds(&DEFAULT_SEEDS),
        raw_replicates: vec![arm(0.780, None)],
        replicate_seed_sets: vec![replicate_seeds(&DEFAULT_SEEDS)],
        paired: None,
    }
}

async fn repo() -> SqliteBenchmarkRepository {
    let pool = setup_test_database().await.expect("setup_test_database");
    sqlx::query(
        "INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key)
         VALUES (1, 'm', '/tmp/m.gguf', 0.5, '2026-01-01 00:00:00', 'm')",
    )
    .execute(&pool)
    .await
    .expect("seed model");
    SqliteBenchmarkRepository::new(pool)
}

/// The apply record must survive both read paths. The row mapper reads
/// `applied_json` with a forgiving `try_get(..).ok()`, so a SELECT that
/// omits the column does not error — it silently reads back `None`, and
/// every Outcome surface renders an em-dash. This is the regression
/// test that makes that omission loud.
#[tokio::test]
async fn applied_json_survives_both_read_paths() {
    let repo = repo().await;
    let run_id = repo
        .create_run(BenchmarkRunType::Tune, &[1], None, None, None)
        .await
        .expect("create run");
    repo.mark_run_applied(run_id, r#"{"verdict":{"verdict":"uncalibrated"}}"#)
        .await
        .expect("mark applied");

    let listed = repo.list_runs(10, 0).await.expect("list");
    assert_eq!(
        listed[0].applied_json.as_deref(),
        Some(r#"{"verdict":{"verdict":"uncalibrated"}}"#),
        "list_runs must select applied_json — the activity surfaces read it from here"
    );

    let fetched = repo.get_run(run_id).await.expect("get").expect("exists");
    assert_eq!(
        fetched.applied_json.as_deref(),
        Some(r#"{"verdict":{"verdict":"uncalibrated"}}"#),
        "get_run must select applied_json — the revert path reads it from here"
    );
}

/// The report is stored whole, so every field a leaderboard reads back —
/// including the ones that distinguish "unmeasured" from "zero" — has to
/// survive the round trip.
#[tokio::test]
async fn an_agentic_report_round_trips() {
    let repo = repo().await;
    let run_id = repo
        .create_run(BenchmarkRunType::Agentic, &[1], None, None, None)
        .await
        .expect("create_run");
    repo.save_agentic_result(&report(), run_id, 1)
        .await
        .expect("save_agentic_result");

    let history = repo
        .get_model_agentic_history(1, 10)
        .await
        .expect("get_model_agentic_history");

    assert_eq!(history.len(), 1);
    let got = &history[0];
    assert_eq!(got.model_name, "qwen2.5-0.5b-instruct");
    assert_eq!(got.raw.loop_avoidance, None, "unmeasured must stay absent");
    assert_eq!(got.gglib.loop_avoidance, Some(1.0));
    assert_eq!(got.raw.total_completion_tokens, Some(226_768));
    assert!((got.delta.wall_time_speedup.unwrap() - 229.83).abs() < 1e-9);
    // The two calibration arms and their sample sizes. A leaderboard that
    // read a stored delta without them would be quoting a magnitude with
    // nothing behind it — the exact reading the arms were added to prevent.
    assert!((got.noise_floor().expect("A/A survived") - 0.022).abs() < 1e-9);
    assert_eq!(got.replicate_seeds, replicate_seeds(&DEFAULT_SEEDS));
    assert_eq!(
        got.control.as_ref().expect("control survived").seeds,
        1,
        "each arm's own seed count, not the run's"
    );
}

/// The run row must come back typed as agentic, not silently fall through
/// the `str_to_run_type` default to `compare`.
#[tokio::test]
async fn an_agentic_run_keeps_its_type() {
    let repo = repo().await;
    let run_id = repo
        .create_run(BenchmarkRunType::Agentic, &[1], None, None, None)
        .await
        .expect("create_run");

    let run = repo.get_run(run_id).await.expect("get_run").expect("some");
    assert_eq!(run.run_type, BenchmarkRunType::Agentic);
}
