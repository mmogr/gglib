//! Tests for [`super::TokenCalibration`].

use super::*;

#[test]
fn unknown_model_returns_static_default() {
    let cal = TokenCalibration::new();
    assert!((cal.chars_per_token("nope") - CHARS_PER_TOKEN_APPROX as f64).abs() < f64::EPSILON);
}

#[test]
fn first_observation_sets_ratio() {
    let cal = TokenCalibration::new();
    // 33000 chars / 10000 tokens = 3.3 chars/token.
    cal.record("m", Some(33_000), 10_000);
    assert!((cal.chars_per_token("m") - 3.3).abs() < 1e-9);
}

#[test]
fn zero_tokens_ignored() {
    let cal = TokenCalibration::new();
    cal.record("m", Some(5_000), 0);
    assert!((cal.chars_per_token("m") - CHARS_PER_TOKEN_APPROX as f64).abs() < f64::EPSILON);
}

/// A request that carried an image reports no characters: its prompt tokens
/// are mostly the image's, and nothing is learned from it.
#[test]
fn a_request_with_an_image_teaches_nothing() {
    let cal = TokenCalibration::new();
    cal.record("m", None, 10_000);
    assert!((cal.chars_per_token("m") - CHARS_PER_TOKEN_APPROX as f64).abs() < f64::EPSILON);

    cal.record("m", Some(33_000), 10_000);
    cal.record("m", None, 1);
    assert!((cal.chars_per_token("m") - 3.3).abs() < 1e-9);
}

#[test]
fn ewma_moves_toward_new_observations() {
    let cal = TokenCalibration::new();
    cal.record("m", Some(40_000), 10_000); // 4.0
    cal.record("m", Some(30_000), 10_000); // 3.0 → EWMA 0.8*4 + 0.2*3 = 3.8
    let r = cal.chars_per_token("m");
    assert!(r < 4.0 && r > 3.0, "ratio {r} should move toward 3.0");
}

#[test]
fn ratio_is_clamped_to_sane_bounds() {
    let cal = TokenCalibration::new();
    // 100 chars / 1 token = 100 → clamped to MAX_RATIO.
    cal.record("hi", Some(100), 1);
    assert!((cal.chars_per_token("hi") - MAX_RATIO).abs() < f64::EPSILON);
    // 1 char / 1000 tokens ≈ 0 → clamped to MIN_RATIO.
    cal.record("lo", Some(1), 1_000);
    assert!((cal.chars_per_token("lo") - MIN_RATIO).abs() < f64::EPSILON);
}

#[test]
fn session_snapshot_is_stable_while_the_global_ratio_keeps_drifting() {
    let cal = TokenCalibration::new();
    let t0 = Instant::now();
    cal.record("m", Some(40_000), 10_000); // global ratio: 4.0
    let frozen = cal.session_chars_per_token("m", "sess-1", t0);

    // More turns land, each updating the global EWMA...
    cal.record("m", Some(30_000), 10_000);
    cal.record("m", Some(20_000), 10_000);
    assert_ne!(
        cal.chars_per_token("m"),
        frozen,
        "the global ratio really did move"
    );

    // ...but this session's snapshot must not.
    assert_eq!(cal.session_chars_per_token("m", "sess-1", t0), frozen);
}

#[test]
fn different_sessions_get_independent_snapshots() {
    let cal = TokenCalibration::new();
    let t0 = Instant::now();
    cal.record("m", Some(40_000), 10_000); // 4.0
    let s1 = cal.session_chars_per_token("m", "sess-1", t0);

    cal.record("m", Some(20_000), 10_000); // pulls global ratio down
    let s2 = cal.session_chars_per_token("m", "sess-2", t0);

    assert_eq!(s1, 4.0);
    assert!(
        s2 < 4.0,
        "sess-2's first snapshot should see the drifted ratio"
    );
    assert_eq!(
        cal.session_chars_per_token("m", "sess-1", t0),
        s1,
        "sess-1 unaffected"
    );
}

#[test]
fn session_snapshot_is_keyed_per_model() {
    let cal = TokenCalibration::new();
    let t0 = Instant::now();
    cal.record("model-a", Some(40_000), 10_000); // 4.0
    cal.record("model-b", Some(20_000), 10_000); // 2.0
    assert_eq!(cal.session_chars_per_token("model-a", "sess-1", t0), 4.0);
    assert_eq!(cal.session_chars_per_token("model-b", "sess-1", t0), 2.0);
}

#[test]
fn clear_session_forces_a_fresh_snapshot() {
    let cal = TokenCalibration::new();
    let t0 = Instant::now();
    cal.record("m", Some(40_000), 10_000);
    let before = cal.session_chars_per_token("m", "sess-1", t0);

    cal.record("m", Some(20_000), 10_000); // drift the global ratio
    cal.clear_session("sess-1");
    let after = cal.session_chars_per_token("m", "sess-1", t0);

    assert_ne!(
        before, after,
        "clearing must pick up the drifted global ratio"
    );
}

#[test]
fn clear_all_sessions_resets_every_session() {
    let cal = TokenCalibration::new();
    let t0 = Instant::now();
    cal.record("m", Some(40_000), 10_000);
    let _ = cal.session_chars_per_token("m", "sess-1", t0);
    let _ = cal.session_chars_per_token("m", "sess-2", t0);

    cal.record("m", Some(20_000), 10_000);
    cal.clear_all_sessions();

    let refreshed = cal.session_chars_per_token("m", "sess-1", t0 + Duration::from_secs(1));
    assert_ne!(refreshed, 4.0);
}

#[test]
fn session_snapshot_expires_after_the_ttl() {
    let cal = TokenCalibration::new();
    let t0 = Instant::now();
    cal.record("m", Some(40_000), 10_000); // 4.0
    let frozen = cal.session_chars_per_token("m", "sess-1", t0);

    cal.record("m", Some(20_000), 10_000); // drift while "frozen"
    let still_frozen = cal.session_chars_per_token("m", "sess-1", t0 + Duration::from_mins(1));
    assert_eq!(still_frozen, frozen, "well within the TTL");

    let after_ttl = cal.session_chars_per_token(
        "m",
        "sess-1",
        t0 + SESSION_SNAPSHOT_TTL + Duration::from_secs(1),
    );
    assert_ne!(
        after_ttl, frozen,
        "TTL expiry must pick up the drifted ratio"
    );
}

#[test]
fn session_snapshots_are_bounded_by_max_session_snapshots() {
    let cal = TokenCalibration::new();
    let t0 = Instant::now();
    cal.record("m", Some(40_000), 10_000);
    for i in 0..(MAX_SESSION_SNAPSHOTS + 10) {
        let _ = cal.session_chars_per_token("m", &format!("sess-{i}"), t0);
    }
    let guard = cal.session_snapshots.lock().unwrap();
    assert!(guard.values.len() <= MAX_SESSION_SNAPSHOTS);
}
