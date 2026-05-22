//! Eval harness for the Director planner — `gglib-agent` orchestrator.
//!
//! Each test in the 20-prompt corpus calls [`plan()`] with a real LLM and
//! records:
//!
//! - Whether the returned [`TaskGraph`] is schema-valid (non-empty, acyclic,
//!   within node/depth limits).
//! - Whether the node goals are semantically coherent (heuristic: each goal
//!   is at least 5 words and different from the top-level goal).
//! - How many replan attempts were made.
//!
//! All tests are gated with `#[ignore]` — run them explicitly with:
//!
//! ```sh
//! cargo test -p gglib-agent --test orchestrator_plan_eval -- --ignored --nocapture
//! ```
//!
//! **Requirements**: a llama-server reachable at the URL in the
//! `GGLIB_EVAL_LLM_URL` environment variable (default:
//! `http://127.0.0.1:9000`).

#![allow(unused_crate_dependencies)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use gglib_agent::orchestrator::plan;
use gglib_core::domain::orchestrator::task_graph::HitlMode;
use gglib_core::ports::LlmCompletionPort;
use gglib_runtime::LlmCompletionAdapter;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn llm_url() -> String {
    std::env::var("GGLIB_EVAL_LLM_URL").unwrap_or_else(|_| "http://127.0.0.1:9000".to_owned())
}

/// Result of a single eval run.
struct EvalResult {
    goal: &'static str,
    valid: bool,
    coherent: bool,
    replan_count: u32,
    node_count: usize,
    error: Option<String>,
}

/// Check that the task graph meets structural and semantic coherence criteria.
fn check_coherence(
    goal: &str,
    graph: &gglib_core::domain::orchestrator::task_graph::TaskGraph,
) -> bool {
    if graph.nodes.is_empty() {
        return false;
    }
    for node in graph.nodes.values() {
        if node.goal.split_whitespace().count() < 5 {
            return false;
        }
        // Node goal must differ from the top-level goal.
        if node.goal.trim().to_lowercase() == goal.trim().to_lowercase() {
            return false;
        }
    }
    true
}

async fn run_eval(goal: &'static str, max_replans: u32) -> EvalResult {
    let replan_count = Arc::new(AtomicU32::new(0));
    let rc = Arc::clone(&replan_count);

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    // Consume events to count replans without blocking the sender.
    tokio::spawn(async move {
        use gglib_core::domain::orchestrator::events::OrchestratorEvent;
        while let Some(ev) = rx.recv().await {
            if matches!(ev, OrchestratorEvent::ReplanAttempt { .. }) {
                rc.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let http_client = reqwest::Client::new();
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(LlmCompletionAdapter::with_client(
        llm_url(),
        http_client,
        None,
    ));

    match plan(goal, &[], llm, HitlMode::None, max_replans, Some(tx)).await {
        Ok(graph) => {
            let valid = true;
            let coherent = check_coherence(goal, &graph);
            let count = replan_count.load(Ordering::Relaxed);
            EvalResult {
                goal,
                valid,
                coherent,
                replan_count: count,
                node_count: graph.nodes.len(),
                error: None,
            }
        }
        Err(e) => EvalResult {
            goal,
            valid: false,
            coherent: false,
            replan_count: replan_count.load(Ordering::Relaxed),
            node_count: 0,
            error: Some(e.to_string()),
        },
    }
}

fn print_result(r: &EvalResult) {
    let status = if r.valid { "✓" } else { "✗" };
    let coherent = if r.coherent { "✓" } else { "✗" };
    println!(
        "[{status}] valid  [{coherent}] coherent  nodes={:2}  replans={:2}  goal=\"{}\"{}",
        r.node_count,
        r.replan_count,
        r.goal,
        r.error
            .as_deref()
            .map(|e| format!("\n    error: {e}"))
            .unwrap_or_default(),
    );
}

#[allow(clippy::cast_precision_loss)]
fn summarise(results: &[EvalResult]) {
    let total = results.len();
    let valid = results.iter().filter(|r| r.valid).count();
    let coherent = results.iter().filter(|r| r.coherent).count();
    let total_replans: u32 = results.iter().map(|r| r.replan_count).sum();
    println!("\n─── Summary ───────────────────────────────────────────────");
    println!(
        "  Valid:    {valid}/{total} ({:.0}%)",
        100.0 * valid as f64 / total as f64,
    );
    println!(
        "  Coherent: {coherent}/{total} ({:.0}%)",
        100.0 * coherent as f64 / total as f64,
    );
    println!(
        "  Avg replans: {:.2}",
        f64::from(total_replans) / total as f64,
    );
    println!("────────────────────────────────────────────────────────────");
}

// ─── 20-prompt corpus ────────────────────────────────────────────────────────

/// Run the full 20-prompt eval corpus.
///
/// Execute with `cargo test -p gglib-agent --test orchestrator_plan_eval -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "requires a running LLM at GGLIB_EVAL_LLM_URL"]
#[allow(clippy::cast_precision_loss)]
async fn eval_full_corpus() {
    const GOALS: &[&str] = &[
        // Research + writing tasks
        "Research the history of llama.cpp and write a summary",
        "Find three recent papers on RAG architectures and draft a literature review",
        "Summarise the changelog of the last five rust-analyzer releases",
        "Collect API pricing for OpenAI, Anthropic, and Mistral then compare them",
        // Multi-step writing pipelines
        "Write a blog post about WebAssembly system interface (WASI) for backend developers",
        "Draft a technical README for a Rust async HTTP client library",
        "Create an outline, draft, and polished final version of a product announcement for a new vector database",
        "Write a one-page executive summary from these three Q3 earnings reports",
        // Code + analysis tasks
        "Analyse security vulnerabilities in the attached auth module and generate a fix patch",
        "Identify performance bottlenecks in this Rust codebase and propose optimisations",
        "Review this Python data-pipeline script and rewrite it using Polars",
        "Generate comprehensive unit tests for the JWT validation function",
        // Research + decision tasks
        "Evaluate three database options for a high-throughput event-sourcing system",
        "Compare serverless vs container-based deployment for a real-time ML inference API",
        "Research the top five vector databases and produce a decision matrix",
        "Survey available open-source LLM fine-tuning frameworks and recommend the best fit for instruction tuning",
        // Mixed pipeline tasks
        "Download the latest NIST NVD CVE feed, parse critical entries, and generate a report",
        "Fetch the GitHub trending repositories for the past week, filter by Rust, and summarise each",
        "Extract key metrics from these five product user interviews and synthesise insights",
        "Plan a three-phase migration from PostgreSQL to CockroachDB, including rollback strategy",
    ];

    let mut results = Vec::with_capacity(GOALS.len());

    println!(
        "\n═══ Orchestrator Plan Eval — {} prompts ═══\n",
        GOALS.len()
    );

    for goal in GOALS {
        let result = run_eval(goal, 2).await;
        print_result(&result);
        results.push(result);
    }

    summarise(&results);

    // Soft assertion — warn but do not fail the test on partial success.
    let valid_pct = results.iter().filter(|r| r.valid).count() as f64 / results.len() as f64;
    assert!(
        valid_pct >= 0.70,
        "Valid rate {:.0}% is below the 70% floor — director quality regression detected",
        valid_pct * 100.0,
    );
}

// ─── Spot-check: single canonical prompt ─────────────────────────────────────

/// Spot-check the canonical acceptance-criteria prompt.
///
/// Verifies that `plan()` returns a valid, non-trivial graph.
#[tokio::test]
#[ignore = "requires a running LLM at GGLIB_EVAL_LLM_URL"]
async fn eval_canonical_acceptance_prompt() {
    let r = run_eval("Research the history of llama.cpp and write a summary", 2).await;
    print_result(&r);
    assert!(
        r.valid,
        "canonical prompt produced an invalid graph: {:?}",
        r.error
    );
    assert!(r.coherent, "canonical prompt produced an incoherent graph");
    assert!(r.node_count >= 2, "expected ≥2 nodes, got {}", r.node_count,);
}
