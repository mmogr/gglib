//! Flag parity between `gglib serve` and `gglib proxy`.
//!
//! `serve` is the pinned mode of the same proxy stack (epic #630), so a cache
//! or sampling knob available on one and not the other is a bug by
//! construction — that divergence is exactly what the epic set out to remove.
//!
//! These assert on the parsed clap model rather than on `--help` text: the
//! flag set is the contract, while the rendered help is formatting. That also
//! keeps the test from failing on wording changes it has no opinion about.
//!
//! The groups are flattened from one shared definition each, so what is
//! really guarded here is that both commands keep flattening them — a
//! regression would take the form of a field moving back inline, or a
//! `#[command(flatten)]` being dropped.

use clap::{CommandFactory, Parser};
use gglib_cli::{Cli, Commands};

/// Every long flag on a subcommand, as clap resolved it.
fn long_flags(subcommand: &str) -> Vec<String> {
    let cli = Cli::command();
    let cmd = cli
        .get_subcommands()
        .find(|c| c.get_name() == subcommand)
        .unwrap_or_else(|| panic!("no `{subcommand}` subcommand"));

    let mut flags: Vec<String> = cmd
        .get_arguments()
        .filter_map(|a| a.get_long().map(str::to_owned))
        .collect();
    flags.sort();
    flags
}

/// The flags `CacheArgs` contributes.
const CACHE_FLAGS: &[&str] = &[
    "cache",
    "cache-disk-gb",
    "cache-ram-mb",
    "cache-reuse",
    "cache-type-k",
    "cache-type-v",
    "slot-dir",
];

/// The flags `AccessArgs` contributes.
const ACCESS_FLAGS: &[&str] = &["allowed-host", "api-key"];

/// The flags `SamplingArgs` contributes.
const SAMPLING_FLAGS: &[&str] = &[
    "max-tokens",
    "min-p",
    "presence-penalty",
    "repeat-penalty",
    "temperature",
    "top-k",
    "top-p",
];

// ─── Parity ───────────────────────────────────────────────────────────────

#[test]
fn serve_and_proxy_expose_the_same_cache_flags() {
    let serve = long_flags("serve");
    let proxy = long_flags("proxy");

    for flag in CACHE_FLAGS {
        assert!(
            serve.contains(&(*flag).to_owned()),
            "`serve` is missing --{flag}; it has {serve:?}"
        );
        assert!(
            proxy.contains(&(*flag).to_owned()),
            "`proxy` is missing --{flag}; it has {proxy:?}"
        );
    }
}

/// An access control available on only one of the two commands would be a
/// hole in whichever lacked it — `serve` is the same proxy stack pinned to one
/// model, and it is reachable over exactly the same network.
#[test]
fn serve_and_proxy_expose_the_same_access_flags() {
    let serve = long_flags("serve");
    let proxy = long_flags("proxy");

    for flag in ACCESS_FLAGS {
        assert!(
            serve.contains(&(*flag).to_owned()),
            "`serve` is missing --{flag}; it has {serve:?}"
        );
        assert!(
            proxy.contains(&(*flag).to_owned()),
            "`proxy` is missing --{flag}; it has {proxy:?}"
        );
    }
}

#[test]
fn serve_and_proxy_expose_the_same_sampling_flags() {
    let serve = long_flags("serve");
    let proxy = long_flags("proxy");

    for flag in SAMPLING_FLAGS {
        assert!(
            serve.contains(&(*flag).to_owned()),
            "`serve` is missing --{flag}; it has {serve:?}"
        );
        assert!(
            proxy.contains(&(*flag).to_owned()),
            "`proxy` is missing --{flag}; it has {proxy:?}"
        );
    }
}

/// `--host` isn't part of a flattened shared-args group — `Serve` and
/// `Proxy` each declare it inline — so nothing else pins this one down.
/// `serve` binding only loopback by default was the original security gap
/// #630 set out to close; without `--host` there is no way to serve a
/// pinned endpoint to another machine on a trusted network at all.
#[test]
fn serve_and_proxy_both_expose_host() {
    let serve = long_flags("serve");
    let proxy = long_flags("proxy");

    assert!(
        serve.contains(&"host".to_owned()),
        "`serve` is missing --host; it has {serve:?}"
    );
    assert!(
        proxy.contains(&"host".to_owned()),
        "`proxy` is missing --host; it has {proxy:?}"
    );
}

/// Guards the rename risk in flattening `SamplingArgs` onto `proxy`: the
/// inline fields derived their long names from the field names, while
/// `SamplingArgs` spells them out explicitly. Anything but an exact match
/// would silently break every existing `gglib proxy` invocation.
#[test]
fn flattening_did_not_rename_any_proxy_flag() {
    let proxy = long_flags("proxy");

    for flag in SAMPLING_FLAGS.iter().chain(CACHE_FLAGS) {
        assert!(
            proxy.contains(&(*flag).to_owned()),
            "--{flag} disappeared from `proxy`; it has {proxy:?}"
        );
    }
}

// ─── Parsing ──────────────────────────────────────────────────────────────

/// The flags must not merely exist — they must land on the right fields.
#[test]
fn serve_parses_the_cache_and_sampling_flags() {
    let cli = Cli::try_parse_from([
        "gglib",
        "serve",
        "1",
        "--cache",
        "--slot-dir",
        "/tmp/slots",
        "--cache-ram-mb",
        "4096",
        "--cache-reuse",
        "256",
        "--cache-disk-gb",
        "8",
        "--cache-type-k",
        "f16",
        "--top-p",
        "0.9",
        "--max-tokens",
        "100",
    ])
    .expect("serve should accept the full cache and sampling flag set");

    let Some(Commands::Serve {
        id,
        cache,
        sampling,
        ..
    }) = cli.command
    else {
        panic!("expected a Serve command");
    };

    assert_eq!(id, 1);
    assert!(cache.cache);
    assert_eq!(
        cache.slot_dir.as_deref(),
        Some(std::path::Path::new("/tmp/slots"))
    );
    assert_eq!(cache.cache_ram_mb, Some(4096));
    assert_eq!(cache.cache_reuse, Some(256));
    assert_eq!(cache.cache_disk_gb, Some(8));
    assert_eq!(
        cache.cache_type_k,
        Some(gglib_core::cache_config::KvCacheType::F16)
    );
    assert_eq!(sampling.top_p, Some(0.9));
    assert_eq!(sampling.max_tokens, Some(100));
}

/// The same invocation `gglib proxy` accepted before the refactor must still
/// parse, and still reach the same fields.
#[test]
fn proxy_still_parses_its_sampling_and_cache_flags() {
    let cli = Cli::try_parse_from([
        "gglib",
        "proxy",
        "--temperature",
        "0.7",
        "--top-p",
        "0.9",
        "--min-p",
        "0.05",
        "--cache",
        "--cache-type-v",
        "q8_0",
    ])
    .expect("proxy should still accept its pre-refactor flags");

    let Some(Commands::Proxy {
        sampling, cache, ..
    }) = cli.command
    else {
        panic!("expected a Proxy command");
    };

    assert_eq!(sampling.temperature, Some(0.7));
    assert_eq!(sampling.top_p, Some(0.9));
    assert_eq!(sampling.min_p, Some(0.05));
    assert!(cache.cache);
    assert_eq!(
        cache.cache_type_v,
        Some(gglib_core::cache_config::KvCacheType::Q8_0)
    );
}

/// Absent flags must stay `None` rather than defaulting: on the sampling
/// side an all-`None` config is what tells the merge hierarchy that the user
/// expressed no opinion, and `--cache` off must mean no cache flags at all.
#[test]
fn omitted_flags_express_no_opinion() {
    let cli = Cli::try_parse_from(["gglib", "serve", "1"]).expect("bare serve should parse");

    let Some(Commands::Serve {
        cache, sampling, ..
    }) = cli.command
    else {
        panic!("expected a Serve command");
    };

    assert!(!cache.cache);
    assert_eq!(cache.slot_dir, None);
    assert_eq!(cache.cache_ram_mb, None);
    assert_eq!(cache.cache_type_k, None);
    assert_eq!(sampling.temperature, None);
    assert_eq!(sampling.top_p, None);
}

/// `model explain` takes an identifier and an optional profile, and the
/// profile must land on the field rather than being swallowed as a second
/// positional.
#[test]
fn model_explain_parses_the_identifier_and_profile() {
    use gglib_cli::model_commands::ModelCommand;

    let cli = Cli::try_parse_from(["gglib", "model", "explain", "3", "--profile", "coding"])
        .expect("model explain should parse");

    let Some(Commands::Model {
        command: ModelCommand::Explain {
            identifier,
            profile,
        },
    }) = cli.command
    else {
        panic!("expected a Model::Explain command");
    };

    assert_eq!(identifier, "3");
    assert_eq!(profile.as_deref(), Some("coding"));
}

/// Without `--profile` the resolution is the unprofiled one, so the field
/// must stay `None` rather than defaulting to some named profile.
#[test]
fn model_explain_leaves_the_profile_unset_when_omitted() {
    use gglib_cli::model_commands::ModelCommand;

    let cli = Cli::try_parse_from(["gglib", "model", "explain", "Qwen3-30B"])
        .expect("bare model explain should parse");

    let Some(Commands::Model {
        command: ModelCommand::Explain { profile, .. },
    }) = cli.command
    else {
        panic!("expected a Model::Explain command");
    };

    assert_eq!(profile, None);
}
