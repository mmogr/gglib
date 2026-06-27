//! Defensive JSON → domain type mappers for benchmark results.
//!
//! All functions treat missing or malformed fields as `None` rather than
//! panicking — the defensive-parsing contract required by the plan.  This
//! module is the single source of truth for extracting data from the raw
//! `serde_json::Value` chunks produced by llama-server's SSE stream and
//! `llama-bench`'s JSON output.
//!
//! # Testing
//!
//! The `#[cfg(test)]` block below is the **executable specification** for the
//! parsing contract.  All 9 tests must pass before any caller may use these
//! functions.

use serde_json::Value;

// ────────────────────────────────────────────────────────────────────────────
// Compare-mode SSE chunk extractors
// ────────────────────────────────────────────────────────────────────────────

/// Extract the streaming text delta from one SSE chunk.
///
/// Returns the content string from `choices[0].delta.content`, or `None` if
/// that path is absent (e.g. the final chunk with `finish_reason`).
pub fn extract_text_delta(val: &Value) -> Option<String> {
    val.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("content"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Extract `finish_reason` from `choices[0].finish_reason`.
///
/// Returns `None` when the field is absent (mid-stream chunks) or not a
/// string.
pub fn extract_finish_reason(val: &Value) -> Option<String> {
    val.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .map(String::from)
}

/// Extract timing fields from the llama-server SSE chunk.
///
/// Returns `(prompt_ms, generation_ms, prompt_tps, generation_tps)`.
/// Every field is independently `None` — a partial `timings` object (e.g.
/// missing `prompt_per_second`) produces `None` only for that field while
/// the rest remain `Some`.
///
/// Field mapping from the llama-server `timings` object:
/// - `prompt_ms`            → `prompt_ms`
/// - `predicted_ms`         → `generation_ms`
/// - `prompt_per_second`    → `prompt_tps`
/// - `predicted_per_second` → `generation_tps`
pub fn extract_compare_timings(
    val: &Value,
) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    let timings = val.get("timings");
    let prompt_ms = timings
        .and_then(|t| t.get("prompt_ms"))
        .and_then(|v| v.as_f64());
    let generation_ms = timings
        .and_then(|t| t.get("predicted_ms"))
        .and_then(|v| v.as_f64());
    let prompt_tps = timings
        .and_then(|t| t.get("prompt_per_second"))
        .and_then(|v| v.as_f64());
    let generation_tps = timings
        .and_then(|t| t.get("predicted_per_second"))
        .and_then(|v| v.as_f64());
    (prompt_ms, generation_ms, prompt_tps, generation_tps)
}

/// Extract token-usage counts from `usage.prompt_tokens` and
/// `usage.completion_tokens`.
///
/// Returns `(prompt_tokens, completion_tokens)`.  Either may be `None` if the
/// field is absent or not an integer.
pub fn extract_usage(val: &Value) -> (Option<i64>, Option<i64>) {
    let usage = val.get("usage");
    let prompt_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_i64());
    let completion_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_i64());
    (prompt_tokens, completion_tokens)
}

// ────────────────────────────────────────────────────────────────────────────
// Perf-mode llama-bench JSON output parser
// ────────────────────────────────────────────────────────────────────────────

/// Parsed output from one `llama-bench -o json` run entry.
///
/// All fields are `Option<_>` so that partial JSON (e.g. a build that omits
/// `t_avg_tg`) can be represented without a hard error.  The caller
/// (`perf.rs`) is responsible for treating missing required fields as a
/// `ModelFailed` event rather than a panic.
#[derive(Debug, Default)]
pub struct PerfBenchOutput {
    /// Average token-generation throughput (tokens/sec) — `t_avg_tg`.
    pub tg_tps: Option<f64>,
    /// Average prompt-processing throughput (tokens/sec) — `t_avg_pp`.
    pub pp_tps: Option<f64>,
    /// Number of generation tokens used — `n_gen`.
    pub n_gen: Option<i64>,
    /// Number of prompt tokens used — `n_prompt`.
    pub n_prompt: Option<i64>,
    /// Backend / model type string — `model_type`.
    pub backend: Option<String>,
    /// Number of GPU layers offloaded — `n_gpu_layers`.
    pub ngl: Option<i64>,
}

/// Parse `llama-bench -o json` stdout into [`PerfBenchOutput`].
///
/// Returns `None` when:
/// - `stdout` is not valid JSON
/// - the top-level value is not a non-empty JSON array
///
/// Individual missing or wrongly-typed fields within an entry produce `None`
/// for that specific field rather than causing the whole parse to fail.
///
/// # Multi-entry output and field-name evolution
///
/// `llama-bench` output format varies across versions:
///
/// **Old format** (single entry, explicit per-metric fields):
/// ```json
/// [{ "t_avg_pp": 200.0, "t_avg_tg": 50.0, "n_prompt": 512, "n_gen": 128 }]
/// ```
///
/// **New format** (two entries, generic `avg_ts`, disambiguated by
/// `n_prompt`/`n_gen`):
/// ```json
/// [
///   { "n_prompt": 512, "n_gen": 0,   "avg_ts": 200.0 },
///   { "n_prompt": 0,   "n_gen": 128, "avg_ts": 50.0  }
/// ]
/// ```
///
/// MTP/draft models (e.g. Qwen3-MTP) always use the two-entry format.
/// The parser handles both: it tries `t_avg_pp`/`t_avg_tg` first, then
/// falls back to `avg_ts` with `n_prompt`/`n_gen` disambiguation.
pub fn parse_perf_output(stdout: &[u8]) -> Option<PerfBenchOutput> {
    let array: Vec<Value> = serde_json::from_slice(stdout).ok()?;
    if array.is_empty() {
        return None;
    }

    let mut tg_tps: Option<f64> = None;
    let mut pp_tps: Option<f64> = None;
    let mut n_gen: Option<i64> = None;
    let mut n_prompt: Option<i64> = None;
    let mut backend: Option<String> = None;
    let mut ngl: Option<i64> = None;

    for entry in &array {
        let entry_n_prompt = entry.get("n_prompt").and_then(|v| v.as_i64()).unwrap_or(0);
        let entry_n_gen = entry.get("n_gen").and_then(|v| v.as_i64()).unwrap_or(0);

        // PP throughput: old-style explicit field, or new-style avg_ts on a
        // prompt-only entry (n_gen == 0).
        if pp_tps.is_none() {
            pp_tps = entry
                .get("t_avg_pp")
                .and_then(|v| v.as_f64())
                .filter(|&v| v > 0.0)
                .or_else(|| {
                    if entry_n_prompt > 0 && entry_n_gen == 0 {
                        entry
                            .get("avg_ts")
                            .and_then(|v| v.as_f64())
                            .filter(|&v| v > 0.0)
                    } else {
                        None
                    }
                });
        }

        // TG throughput: old-style explicit field, or new-style avg_ts on a
        // generation-only entry (n_prompt == 0).
        if tg_tps.is_none() {
            tg_tps = entry
                .get("t_avg_tg")
                .and_then(|v| v.as_f64())
                .filter(|&v| v > 0.0)
                .or_else(|| {
                    if entry_n_gen > 0 && entry_n_prompt == 0 {
                        entry
                            .get("avg_ts")
                            .and_then(|v| v.as_f64())
                            .filter(|&v| v > 0.0)
                    } else {
                        None
                    }
                });
        }

        if n_gen.is_none() {
            n_gen = Some(entry_n_gen).filter(|&v| v > 0);
        }
        if n_prompt.is_none() {
            n_prompt = Some(entry_n_prompt).filter(|&v| v > 0);
        }
        if backend.is_none() {
            backend = entry
                .get("model_type")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
        if ngl.is_none() {
            ngl = entry.get("n_gpu_layers").and_then(|v| v.as_i64());
        }
    }

    Some(PerfBenchOutput {
        tg_tps,
        pp_tps,
        n_gen,
        n_prompt,
        backend,
        ngl,
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Tests — executable specification for the defensive-parsing contract
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Compare timing tests ─────────────────────────────────────────────────

    /// All timing fields present → every `Option` is `Some`.
    #[test]
    fn test_compare_timing_complete() {
        let val = json!({
            "timings": {
                "prompt_ms": 50.0,
                "predicted_ms": 1000.0,
                "prompt_per_second": 200.0,
                "predicted_per_second": 100.0
            },
            "usage": { "prompt_tokens": 10, "completion_tokens": 100 }
        });

        let (prompt_ms, gen_ms, prompt_tps, gen_tps) = extract_compare_timings(&val);
        assert_eq!(prompt_ms, Some(50.0));
        assert_eq!(gen_ms, Some(1000.0));
        assert_eq!(prompt_tps, Some(200.0));
        assert_eq!(gen_tps, Some(100.0));

        let (pt, ct) = extract_usage(&val);
        assert_eq!(pt, Some(10));
        assert_eq!(ct, Some(100));
    }

    /// `timings` present but one field (`prompt_per_second`) absent → that
    /// field returns `None`; all other fields remain `Some`.
    #[test]
    fn test_compare_timing_missing_field() {
        let val = json!({
            "timings": {
                "prompt_ms": 50.0,
                "predicted_ms": 1000.0,
                "predicted_per_second": 100.0
            }
        });

        let (prompt_ms, gen_ms, prompt_tps, gen_tps) = extract_compare_timings(&val);
        assert_eq!(prompt_ms, Some(50.0));
        assert_eq!(gen_ms, Some(1000.0));
        assert_eq!(prompt_tps, None, "absent prompt_per_second → None");
        assert_eq!(gen_tps, Some(100.0));
    }

    /// `"timings": null` → all timing fields `None`, no panic.
    #[test]
    fn test_compare_timing_null_object() {
        let val = json!({ "timings": null });

        let (prompt_ms, gen_ms, prompt_tps, gen_tps) = extract_compare_timings(&val);
        assert_eq!(prompt_ms, None);
        assert_eq!(gen_ms, None);
        assert_eq!(prompt_tps, None);
        assert_eq!(gen_tps, None);
    }

    /// No `timings` key at all → all timing fields `None`.
    #[test]
    fn test_compare_timing_absent_key() {
        let val = json!({ "choices": [{ "delta": { "content": "hello" } }] });

        let (prompt_ms, gen_ms, prompt_tps, gen_tps) = extract_compare_timings(&val);
        assert_eq!(prompt_ms, None);
        assert_eq!(gen_ms, None);
        assert_eq!(prompt_tps, None);
        assert_eq!(gen_tps, None);
    }

    /// One timing field has the wrong type (string instead of number) → that
    /// field returns `None`; sibling fields still parse correctly.
    #[test]
    fn test_compare_timing_wrong_type() {
        let val = json!({
            "timings": {
                "prompt_per_second": "fast",
                "predicted_per_second": 100.0,
                "prompt_ms": 50.0,
                "predicted_ms": 1000.0
            }
        });

        let (prompt_ms, gen_ms, prompt_tps, gen_tps) = extract_compare_timings(&val);
        assert_eq!(prompt_ms, Some(50.0));
        assert_eq!(gen_ms, Some(1000.0));
        assert_eq!(prompt_tps, None, "wrong-type prompt_per_second → None");
        assert_eq!(gen_tps, Some(100.0));
    }

    /// `finish_reason == "length"` → `was_truncated` flag should be `true`.
    #[test]
    fn test_compare_finish_reason_length() {
        let val = json!({ "choices": [{ "finish_reason": "length" }] });

        let reason = extract_finish_reason(&val);
        let was_truncated = reason.as_deref() == Some("length");
        assert!(was_truncated, "finish_reason=length → was_truncated=true");
    }

    /// `finish_reason == "stop"` → `was_truncated` flag should be `false`.
    #[test]
    fn test_compare_finish_reason_stop() {
        let val = json!({ "choices": [{ "finish_reason": "stop" }] });

        let reason = extract_finish_reason(&val);
        let was_truncated = reason.as_deref() == Some("length");
        assert!(!was_truncated, "finish_reason=stop → was_truncated=false");
    }

    // ── Perf output tests ────────────────────────────────────────────────────

    /// Valid `llama-bench -o json` output → all `PerfBenchOutput` fields
    /// populated.
    #[test]
    fn test_perf_bench_output_complete() {
        let stdout = serde_json::to_vec(&json!([{
            "t_avg_tg": 52.3,
            "t_avg_pp": 214.5,
            "model_type": "llama 7B Q4_0",
            "n_gen": 128,
            "n_prompt": 512,
            "n_gpu_layers": 32
        }]))
        .unwrap();

        let out = parse_perf_output(&stdout).expect("should parse successfully");
        assert_eq!(out.tg_tps, Some(52.3));
        assert_eq!(out.pp_tps, Some(214.5));
        assert_eq!(out.backend, Some("llama 7B Q4_0".to_string()));
        assert_eq!(out.n_gen, Some(128));
        assert_eq!(out.n_prompt, Some(512));
        assert_eq!(out.ngl, Some(32));
    }

    /// Partial JSON output — `t_avg_tg` missing → `tg_tps` is `None`; other
    /// fields are still populated; no panic.
    #[test]
    fn test_perf_bench_output_missing_field() {
        let stdout = serde_json::to_vec(&json!([{
            "t_avg_pp": 214.5,
            "model_type": "llama 7B Q4_0",
            "n_gen": 128,
            "n_prompt": 512
        }]))
        .unwrap();

        let out = parse_perf_output(&stdout).expect("should still parse");
        assert_eq!(out.tg_tps, None, "missing t_avg_tg → tg_tps is None");
        assert_eq!(out.pp_tps, Some(214.5));
        assert_eq!(out.backend, Some("llama 7B Q4_0".to_string()));
    }

    /// MTP / draft model output: `llama-bench` emits two entries using the
    /// new `avg_ts` format — a PP-only entry (`n_gen=0`) followed by a
    /// TG-only entry (`n_prompt=0`).  Both fields must be gathered correctly.
    #[test]
    fn test_perf_bench_mtp_split_entries() {
        let stdout = serde_json::to_vec(&json!([
            // PP-only entry (n_gen=0) — new avg_ts format
            {
                "model_type": "qwen35moe 35B.A3B Q8_0",
                "n_prompt": 512,
                "n_gen": 0,
                "avg_ts": 180.0,
                "n_gpu_layers": -1
            },
            // TG-only entry (n_prompt=0) — new avg_ts format
            {
                "model_type": "qwen35moe 35B.A3B Q8_0",
                "n_prompt": 0,
                "n_gen": 128,
                "avg_ts": 45.7,
                "n_gpu_layers": -1
            }
        ]))
        .unwrap();

        let out = parse_perf_output(&stdout).expect("should parse MTP split output");
        assert_eq!(out.tg_tps, Some(45.7), "tg_tps from TG entry avg_ts");
        assert_eq!(out.pp_tps, Some(180.0), "pp_tps from PP entry avg_ts");
        assert_eq!(out.n_prompt, Some(512));
        assert_eq!(out.n_gen, Some(128));
        assert_eq!(out.backend, Some("qwen35moe 35B.A3B Q8_0".to_string()));
        assert_eq!(out.ngl, Some(-1));
    }

    /// Old-format single entry with explicit t_avg_pp / t_avg_tg still parses.
    #[test]
    fn test_perf_bench_old_format_single_entry() {
        let stdout = serde_json::to_vec(&json!([{
            "t_avg_pp": 200.0,
            "t_avg_tg": 55.0,
            "model_type": "llama 7B Q4_0",
            "n_prompt": 512,
            "n_gen": 128,
            "n_gpu_layers": 32
        }]))
        .unwrap();

        let out = parse_perf_output(&stdout).expect("should parse old format");
        assert_eq!(out.pp_tps, Some(200.0));
        assert_eq!(out.tg_tps, Some(55.0));
    }

    /// Empty JSON array → `None` returned (not a panic).
    #[test]
    fn test_perf_bench_empty_array() {
        let stdout = b"[]";
        assert!(parse_perf_output(stdout).is_none());
    }
}
