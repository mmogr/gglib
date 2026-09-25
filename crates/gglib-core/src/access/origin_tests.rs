use super::may_change;
use crate::CorsConfig;

/// The daemon's `Host` on its default loopback bind.
const DAEMON: &str = "127.0.0.1:9887";

/// The four origins the daemon lets read by default: the desktop app's web
/// view on each platform and the Vite dev server.
const DESKTOP_AND_DEV: [&str; 4] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:5173",
];

fn the_daemons_default() -> CorsConfig {
    CorsConfig::AllowOrigins(DESKTOP_AND_DEV.map(str::to_owned).to_vec())
}

/// A config that lets no page read, so only the same-origin rule can admit.
fn nobody_reads() -> CorsConfig {
    CorsConfig::AllowOrigins(Vec::new())
}

#[test]
fn a_local_origin_may_change_things_on_any_port_where_local_pages_may_read() {
    for origin in [
        "http://localhost:3000",
        "http://127.0.0.1:1",
        "http://[::1]:8080",
        "tauri://localhost",
    ] {
        assert!(
            may_change(&CorsConfig::LocalOnly, Some(origin), None, DAEMON),
            "{origin}"
        );
    }
}

#[test]
fn an_origin_naming_the_host_it_was_sent_to_may_change_things() {
    let cors = nobody_reads();
    assert!(may_change(
        &cors,
        Some("http://127.0.0.1:9887"),
        None,
        DAEMON
    ));
    assert!(may_change(
        &cors,
        Some("http://GGLIB.local:9887"),
        None,
        "gglib.local:9887"
    ));
    assert!(may_change(
        &cors,
        Some("https://gglib.test"),
        None,
        "gglib.test"
    ));
}

#[test]
fn the_same_name_on_another_port_is_another_site() {
    let cors = nobody_reads();
    assert!(!may_change(
        &cors,
        Some("http://127.0.0.1:9888"),
        None,
        DAEMON
    ));
    assert!(!may_change(&cors, Some("http://127.0.0.1"), None, DAEMON));
    assert!(!may_change(
        &cors,
        Some("http://127.0.0.1:98870"),
        None,
        DAEMON
    ));
}

#[test]
fn only_a_web_page_can_name_the_host_it_was_sent_to() {
    assert!(!may_change(
        &nobody_reads(),
        Some("ftp://127.0.0.1:9887"),
        None,
        DAEMON
    ));
}

#[test]
fn a_page_that_hides_its_origin_may_change_nothing_even_where_every_page_may_read() {
    for origin in ["null", "", "127.0.0.1:9887", "http://", "://127.0.0.1:9887"] {
        assert!(
            !may_change(&CorsConfig::AllowAll, Some(origin), None, DAEMON),
            "{origin:?}"
        );
    }
}

#[test]
fn fetch_metadata_decides_only_when_there_is_no_origin() {
    let cors = CorsConfig::LocalOnly;
    assert!(!may_change(&cors, None, Some("cross-site"), DAEMON));
    assert!(!may_change(&cors, None, Some("Cross-Site"), DAEMON));
    for site in [None, Some("same-origin"), Some("same-site"), Some("none")] {
        assert!(may_change(&cors, None, site, DAEMON), "{site:?}");
    }
    // The desktop app is cross-site by its metadata and admitted by origin.
    assert!(may_change(
        &the_daemons_default(),
        Some("tauri://localhost"),
        Some("cross-site"),
        DAEMON
    ));
    // A cross-site origin is not rescued by metadata that says otherwise.
    assert!(!may_change(
        &cors,
        Some("https://evil.example"),
        Some("same-origin"),
        DAEMON
    ));
}

/// The invariant the guard exists to keep, over every config there is: a
/// page that names any origin but the endpoint's own may change something
/// exactly when the CORS layer lets it read the answer. The host names none
/// of these origins, so the same-origin rule is out of play, and each
/// config's readers are spelled out so that a change to what a config lets
/// read fails here rather than moving both sides together.
/// `tauri.localhost.evil.com` and `localhost:51730` extend an origin on the
/// daemon's list, so a list matched by prefix fails here too.
#[test]
fn every_origin_a_config_lets_read_may_write_and_no_other() {
    const ORIGINS: [&str; 15] = [
        "https://evil.example",
        "http://evil.com:9887",
        "http://localhost.evil.com",
        "http://tauri.localhost.evil.com",
        "http://192.168.1.5:9887",
        "http://user@localhost",
        "http://localhost:3000",
        "http://localhost:5173",
        "http://localhost:51730",
        "http://127.0.0.1:8080",
        "http://[::1]:9",
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
        "asset://localhost",
    ];
    let local: &[&str] = &ORIGINS[6..];
    let cases: [(CorsConfig, &[&str]); 3] = [
        (CorsConfig::AllowAll, &ORIGINS),
        (the_daemons_default(), &DESKTOP_AND_DEV),
        (CorsConfig::LocalOnly, local),
    ];
    for (cors, readers) in cases {
        for origin in ORIGINS {
            let reads = cors.allows_origin(origin);
            assert_eq!(
                reads,
                readers.contains(&origin),
                "{cors:?} lets {origin} read"
            );
            assert_eq!(
                may_change(&cors, Some(origin), None, "gglib.test:9887"),
                reads,
                "{cors:?}: {origin} may write iff it may read"
            );
        }
    }
}
