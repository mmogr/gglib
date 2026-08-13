//! Tests for [`super`] — the dashboard's frame rendering.
//!
//! Split into their own file the way `models_tests.rs` and `queue_tests.rs`
//! are: the renderer is ~430 lines and its tests are half again as many, since
//! nearly every rule about what the dashboard says is asserted here rather
//! than by reading the code.

use super::super::DEFAULT_TERM_WIDTH;
use super::super::wire::*;
use super::*;

#[test]
fn progress_bar_renders_full_and_empty() {
    assert_eq!(
        progress_bar(0, 100, 10),
        "[\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}]   0%"
    );
    assert_eq!(
        progress_bar(100, 100, 10),
        "[\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}] 100%"
    );
}

#[test]
fn progress_bar_zero_total_is_empty_not_a_panic() {
    assert_eq!(progress_bar(5, 0, 10), progress_bar(0, 100, 10));
}

#[test]
fn progress_bar_rounds_to_nearest_cell() {
    // 5/10 = 50% of a 4-cell bar -> 2 filled cells.
    assert_eq!(
        progress_bar(5, 10, 4),
        "[\u{2588}\u{2588}\u{2591}\u{2591}]  50%"
    );
}

#[test]
fn format_elapsed_secs_under_a_minute() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(format_elapsed_secs(now - 5), "5s");
}

#[test]
fn format_elapsed_secs_over_a_minute() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(format_elapsed_secs(now - 125), "2m 5s");
}

#[test]
fn truncate_leaves_short_strings_unchanged() {
    assert_eq!(truncate("qwen3", 24), "qwen3");
}

#[test]
fn truncate_cuts_long_strings_with_ellipsis() {
    let result = truncate("a-very-long-model-name-that-overflows", 10);
    assert_eq!(result.chars().count(), 10);
    assert!(result.ends_with('\u{2026}'));
}

#[test]
fn render_frame_shows_placeholder_when_no_connections() {
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: Some("disabled upstream (--no-slots)".to_string()),
        total_requests: 0,
        cache: None,
        agent_usage: CacheUsage::default(),
        admission: AdmissionSnapshot::default(),
        per_model_defects: BTreeMap::new(),
    };
    let frame = render_frame(
        "http://127.0.0.1:8080/v1/proxy/status/stream",
        &snapshot,
        DEFAULT_TERM_WIDTH,
    );
    assert!(frame.contains("(none)"));
    assert!(frame.contains("disabled upstream (--no-slots)"));
    assert!(frame.contains("Total requests served: 0"));
}

#[test]
fn render_frame_shows_connection_and_slot_bars() {
    let snapshot = DashboardSnapshot {
        active_connections: vec![ActiveConnectionSnapshot {
            model_name: "qwen3-30b".to_string(),
            started_at_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            phase: ConnectionPhase::ProcessingPrompt,
            prompt_processed: Some(50),
            prompt_total: Some(100),
        }],
        slots_available: true,
        slots: vec![
            serde_json::from_str(r#"{"id": 0, "n_ctx": 4096, "n_past": 2048}"#)
                .expect("should parse"),
        ],
        slots_status: None,
        total_requests: 3,
        cache: None,
        agent_usage: CacheUsage::default(),
        admission: AdmissionSnapshot::default(),
        per_model_defects: BTreeMap::new(),
    };
    let frame = render_frame(
        "http://127.0.0.1:8080/v1/proxy/status/stream",
        &snapshot,
        DEFAULT_TERM_WIDTH,
    );
    assert!(frame.contains("qwen3-30b"));
    assert!(frame.contains("prompt"));
    assert!(frame.contains("50%")); // 50/100 prompt progress
    assert!(frame.contains("slot 0"));
    assert!(frame.contains("Total requests served: 3"));
}

#[test]
fn render_frame_truncates_long_slots_error_to_fit_terminal_width() {
    // A realistic reqwest connect-error string easily exceeds 100 chars
    // — e.g. "error sending request for url (http://127.0.0.1:5500/slots):
    // error trying to connect: tcp connect error: Connection refused (os
    // error 61)". This still confirms the pre-truncation keeps the line
    // within one row, on top of the general wrap-aware row counting in
    // `visual_row_count`.
    let long_reason = "error sending request for url (http://127.0.0.1:5500/slots): ".to_string()
        + &"error trying to connect: tcp connect error: Connection refused ".repeat(3);
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: Some(long_reason.clone()),
        total_requests: 0,
        cache: None,
        agent_usage: CacheUsage::default(),
        admission: AdmissionSnapshot::default(),
        per_model_defects: BTreeMap::new(),
    };
    let width = 80u16;
    let frame = render_frame(
        "http://127.0.0.1:8080/v1/proxy/status/stream",
        &snapshot,
        width,
    );

    assert!(
        long_reason.chars().count() as u16 > width,
        "test fixture must actually exceed the terminal width"
    );
    for line in frame.lines() {
        assert!(
            line.chars().count() <= width as usize,
            "line exceeds terminal width ({} > {width}): {line:?}",
            line.chars().count()
        );
    }
    assert!(
        frame.contains('\u{2026}'),
        "long reason should be truncated with an ellipsis"
    );
}

#[test]
fn visual_row_count_matches_logical_lines_when_nothing_wraps() {
    let frame = "gglib proxy dashboard\n(Ctrl+C to exit)\n\nTotal requests served: 0\n";
    assert_eq!(
        visual_row_count(frame, DEFAULT_TERM_WIDTH),
        frame.lines().count() as u16
    );
}

#[test]
fn visual_row_count_counts_a_wrapped_line_as_multiple_rows() {
    let frame = format!("{}\n", "x".repeat(150));
    assert_eq!(visual_row_count(&frame, 80), 2);
}

/// Reproduces the reported bug directly: at a narrow terminal width the
/// unguarded header line (fixed content, no truncation applied to it)
/// is long enough to wrap onto a second physical row. The old
/// `frame.lines().count()` redraw math would undercount here — proving
/// exactly the undershoot that made the dashboard drift/repeat down a
/// narrow terminal instead of redrawing in place.
#[test]
fn visual_row_count_exceeds_naive_line_count_on_a_narrow_terminal() {
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: None,
        total_requests: 0,
        cache: None,
        agent_usage: CacheUsage::default(),
        admission: AdmissionSnapshot::default(),
        per_model_defects: BTreeMap::new(),
    };
    let term_width = 40u16;
    let frame = render_frame(
        "http://127.0.0.1:8080/v1/proxy/status/stream",
        &snapshot,
        term_width,
    );

    let naive_count = frame.lines().count() as u16;
    let accurate_count = visual_row_count(&frame, term_width);
    assert!(
        accurate_count > naive_count,
        "expected wrapping to be detected: naive={naive_count} accurate={accurate_count}"
    );
}

// ── Prompt cache section ─────────────────────────────────────────────

fn cache_status(usage: CacheUsage) -> CacheStatus {
    CacheStatus {
        disk_enabled: true,
        disk_suppressed_for_model: false,
        ram_budget_mb: Some(70_008),
        ram_state: "healthy".to_string(),
        warnings: vec![],
        usage,
    }
}

fn frame_with_cache(cache: Option<CacheStatus>) -> String {
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: None,
        total_requests: 3,
        cache,
        agent_usage: CacheUsage::default(),
        admission: AdmissionSnapshot::default(),
        per_model_defects: BTreeMap::new(),
    };
    render_frame("http://127.0.0.1:8080", &snapshot, DEFAULT_TERM_WIDTH)
}

fn frame_with_agent_usage(agent_usage: CacheUsage) -> String {
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: None,
        total_requests: 0,
        cache: None,
        agent_usage,
        admission: AdmissionSnapshot::default(),
        per_model_defects: BTreeMap::new(),
    };
    render_frame("http://127.0.0.1:8080", &snapshot, DEFAULT_TERM_WIDTH)
}

fn frame_with_admission(admission: AdmissionSnapshot) -> String {
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: None,
        total_requests: 0,
        cache: None,
        agent_usage: CacheUsage::default(),
        admission,
        per_model_defects: BTreeMap::new(),
    };
    render_frame("http://127.0.0.1:8080", &snapshot, DEFAULT_TERM_WIDTH)
}

/// A proxy that predates admission control still renders — the section
/// reports an empty resident set rather than the frame losing a panel.
#[test]
fn admission_section_renders_an_empty_resident_set() {
    let frame = frame_with_admission(AdmissionSnapshot::default());
    assert!(frame.contains("VRAM residency"), "{frame}");
    assert!(frame.contains("(no model loaded)"), "{frame}");
    assert!(frame.contains("Model swaps"), "{frame}");
}

#[test]
fn admission_section_names_each_resident_and_its_role() {
    let frame = frame_with_admission(AdmissionSnapshot {
        slots: vec![
            ResidentSlotSnapshot {
                model_name: "qwen-coder".to_string(),
                inflight: 2,
                is_primary: true,
                resident_for_secs: 95,
            },
            ResidentSlotSnapshot {
                model_name: "nomic-embed".to_string(),
                inflight: 0,
                is_primary: false,
                resident_for_secs: 30,
            },
        ],
        ..Default::default()
    });

    assert!(frame.contains("qwen-coder"), "{frame}");
    assert!(frame.contains("primary"), "{frame}");
    assert!(frame.contains("2 in flight"), "{frame}");
    assert!(frame.contains("1m 35s"), "{frame}");
    assert!(frame.contains("nomic-embed"), "{frame}");
    assert!(frame.contains("secondary"), "{frame}");
    assert!(frame.contains("idle"), "{frame}");
}

/// The whole reason the server sends prose: an idle second slot has to
/// explain itself, or a user with free VRAM reads it as a bug.
#[test]
fn admission_section_prints_the_second_slot_explanation() {
    let frame = frame_with_admission(AdmissionSnapshot {
        secondary_slot: SecondarySlotStatus {
            detail: "Not enough free VRAM to keep a second model loaded.".to_string(),
        },
        ..Default::default()
    });

    assert!(frame.contains("Not enough free VRAM"), "{frame}");
}

#[test]
fn admission_section_reports_queue_depth_and_the_oldest_wait() {
    let frame = frame_with_admission(AdmissionSnapshot {
        queued: vec![QueuedModelSnapshot {
            model_name: "nomic-embed".to_string(),
            waiting: 4,
            oldest_wait_ms: 95_000,
        }],
        total_swaps: 3,
        ..Default::default()
    });

    assert!(frame.contains("4 waiting"), "{frame}");
    assert!(frame.contains("oldest 1m 35s"), "{frame}");
    assert!(frame.contains("Model swaps"), "{frame}");
    assert!(frame.contains('3'), "{frame}");
}

/// A server-phrased explanation can be arbitrarily long; it must not wrap
/// and break the cursor arithmetic the redraw depends on.
#[test]
fn a_long_second_slot_explanation_is_clipped_to_the_terminal() {
    let detail = "x".repeat(400);
    let snapshot = DashboardSnapshot {
        active_connections: vec![],
        slots_available: false,
        slots: vec![],
        slots_status: None,
        total_requests: 0,
        cache: None,
        agent_usage: CacheUsage::default(),
        admission: AdmissionSnapshot {
            secondary_slot: SecondarySlotStatus { detail },
            ..Default::default()
        },
        per_model_defects: BTreeMap::new(),
    };

    let frame = render_frame("http://127.0.0.1:8080", &snapshot, 80);
    for line in frame.lines() {
        assert!(
            line.chars().count() <= 80,
            "line exceeds the terminal width: {line}"
        );
    }
}

#[test]
fn cache_section_reports_when_no_model_has_resolved() {
    let frame = frame_with_cache(None);
    assert!(frame.contains("Prompt cache"));
    assert!(frame.contains("(no model resolved yet)"), "{frame}");
}

/// The agent population renders in its own section, even when no proxied
/// model has resolved (so the "Prompt cache" section shows the placeholder).
#[test]
fn agent_cache_section_renders_its_own_population() {
    let idle = frame_with_agent_usage(CacheUsage::default());
    assert!(idle.contains("Agent cache (GUI chat)"), "{idle}");
    assert!(idle.contains("(no cache activity recorded yet)"), "{idle}");
    // The proxied section is independent and still shows its placeholder.
    assert!(idle.contains("(no model resolved yet)"), "{idle}");

    let active = frame_with_agent_usage(CacheUsage {
        reporting_requests: 4,
        prompt_tokens: 12_000,
        cached_tokens: 9_800,
        last_prompt_tokens: Some(3_000),
        last_cached_tokens: Some(2_500),
        ..CacheUsage::default()
    });
    assert!(active.contains("Agent cache (GUI chat)"), "{active}");
    assert!(active.contains("9,800 of 12,000 prompt tokens"), "{active}");
    assert!(
        active.contains("2,500 of 3,000 tokens from cache"),
        "{active}"
    );
}

#[test]
fn cache_section_shows_reuse_totals_with_separators() {
    let frame = frame_with_cache(Some(cache_status(CacheUsage {
        reporting_requests: 3,
        prompt_tokens: 30_342,
        cached_tokens: 29_450,
        last_prompt_tokens: Some(10_000),
        last_cached_tokens: Some(9_500),
        ..CacheUsage::default()
    })));
    assert!(frame.contains("29,450 of 30,342 prompt tokens"), "{frame}");
    assert!(
        frame.contains("9,500 of 10,000 tokens from cache"),
        "{frame}"
    );
    assert!(frame.contains("RAM budget: 70,008 MiB"), "{frame}");
    assert!(frame.contains("disk: on"), "{frame}");
}

/// "Nothing measured yet" and "measured, and it was zero" are different
/// facts; the server keeps them apart, so the frame must too.
#[test]
fn cache_section_distinguishes_no_activity_from_a_measured_zero() {
    // Scope to the proxied "Prompt cache" section: the agent section shares
    // the same placeholder text and would otherwise mask the distinction.
    let proxied = |frame: &str| frame.split("Agent cache").next().unwrap().to_string();

    let idle = proxied(&frame_with_cache(Some(cache_status(CacheUsage::default()))));
    assert!(idle.contains("(no cache activity recorded yet)"), "{idle}");

    let measured_zero = proxied(&frame_with_cache(Some(cache_status(CacheUsage {
        reporting_requests: 1,
        prompt_tokens: 5_000,
        cached_tokens: 0,
        last_prompt_tokens: Some(5_000),
        last_cached_tokens: Some(0),
        ..CacheUsage::default()
    }))));
    assert!(
        !measured_zero.contains("no cache activity"),
        "{measured_zero}"
    );
    assert!(
        measured_zero.contains("0 of 5,000 prompt tokens"),
        "{measured_zero}"
    );
}

#[test]
fn cache_section_renders_server_warnings() {
    let mut cache = cache_status(CacheUsage::default());
    cache.warnings = vec!["Low memory available for prompt caching.".to_string()];
    let frame = frame_with_cache(Some(cache));
    assert!(frame.contains("! Low memory available"), "{frame}");
}

/// Warnings are server-phrased and can be long; they must not wrap the
/// frame onto extra physical rows, which would corrupt the redraw's
/// line-count arithmetic.
#[test]
fn cache_section_truncates_a_long_warning_to_one_row() {
    let mut cache = cache_status(CacheUsage::default());
    cache.warnings = vec!["w".repeat(500)];
    let frame = frame_with_cache(Some(cache));
    let longest = frame.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    assert!(
        longest <= usize::from(DEFAULT_TERM_WIDTH),
        "longest line was {longest} columns"
    );
}

#[test]
fn cache_section_names_a_model_suppressed_disk_layer() {
    let mut cache = cache_status(CacheUsage::default());
    cache.disk_suppressed_for_model = true;
    let frame = frame_with_cache(Some(cache));
    assert!(frame.contains("disk: off for this model"), "{frame}");
}

#[test]
fn cache_section_omits_the_budget_when_llama_default_applies() {
    let mut cache = cache_status(CacheUsage::default());
    cache.ram_state = "llama_default".to_string();
    cache.ram_budget_mb = None;
    let frame = frame_with_cache(Some(cache));
    assert!(!frame.contains("RAM budget"), "{frame}");
    assert!(frame.contains("disk: on"), "{frame}");
}

#[test]
fn cache_section_explains_a_budget_the_machine_cannot_afford() {
    let mut cache = cache_status(CacheUsage::default());
    cache.ram_state = "disabled_insufficient_ram".to_string();
    cache.ram_budget_mb = Some(0);
    let frame = frame_with_cache(Some(cache));
    assert!(frame.contains("not enough memory"), "{frame}");
}

/// A permanent "0" row would be noise on any current llama.cpp.
#[test]
fn cache_section_hides_the_no_data_row_unless_it_is_non_zero() {
    let none_missing = frame_with_cache(Some(cache_status(CacheUsage {
        reporting_requests: 1,
        ..CacheUsage::default()
    })));
    assert!(!none_missing.contains("No cache data"), "{none_missing}");

    let some_missing = frame_with_cache(Some(cache_status(CacheUsage {
        reporting_requests: 1,
        unreported_requests: 2,
        ..CacheUsage::default()
    })));
    assert!(some_missing.contains("No cache data"), "{some_missing}");
}

#[test]
fn thousands_inserts_separators_at_the_right_boundaries() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1_000), "1,000");
    assert_eq!(thousands(70_008), "70,008");
    assert_eq!(thousands(1_234_567), "1,234,567");
    assert_eq!(thousands(u64::MAX), "18,446,744,073,709,551,615");
}

// ── Defects section ──────────────────────────────────────────────────────

fn counts(build: impl FnOnce(&mut ModelDefectCounts)) -> ModelDefectCounts {
    let mut c = ModelDefectCounts {
        requests: 100,
        ..Default::default()
    };
    build(&mut c);
    c
}

/// Before anything has been forwarded there is no evidence either way,
/// and "none" would be a claim this run has not earned.
#[test]
fn defects_distinguish_no_evidence_from_a_clean_run() {
    let empty = render_defects_section(&BTreeMap::new());
    assert!(empty.contains("nothing recorded yet"), "{empty}");

    let clean = BTreeMap::from([("qwen".to_string(), counts(|_| {}))]);
    let rendered = render_defects_section(&clean);
    assert!(
        rendered.contains("none across 100 request(s)"),
        "{rendered}"
    );
}

/// A healthy model produces zero of every counter, so listing it would
/// bury the one model that has something wrong with it.
#[test]
fn only_models_with_something_to_report_get_a_line() {
    let per_model = BTreeMap::from([
        ("healthy-model".to_string(), counts(|_| {})),
        ("sick-model".to_string(), counts(|c| c.stream_errors = 3)),
    ]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("sick-model"), "{rendered}");
    assert!(!rendered.contains("healthy-model"), "{rendered}");
    assert!(rendered.contains("stream errors"), "{rendered}");
}

/// The ratio is the number worth watching: attempts say how often this
/// model packages a call badly, successes whether the one lever works.
#[test]
fn repairs_are_shown_as_a_ratio() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.repairs_attempted = 9;
            c.repairs_succeeded = 7;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("7 of 9 succeeded"), "{rendered}");
}

/// `reasoning_only` is counted *within* `empty_responses`, not beside it.
/// Printing them as peers reads as more empty turns than happened.
#[test]
fn reasoning_only_turns_are_shown_inside_the_empty_total() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.empty_responses = 4;
            c.reasoning_only = 3;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(
        rendered.contains("empty responses          4 (3 reasoning-only)"),
        "{rendered}"
    );
}

/// A counter at zero is not news. Only what fired is printed, or a model
/// with one bad turn costs eight lines of zeroes.
#[test]
fn a_counter_that_never_fired_is_not_printed() {
    let per_model = BTreeMap::from([("qwen".to_string(), counts(|c| c.dialect_residue = 1))]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("dialect residue"), "{rendered}");
    assert!(!rendered.contains("loop-guard trips"), "{rendered}");
    assert!(!rendered.contains("truncated at ceiling"), "{rendered}");
}
