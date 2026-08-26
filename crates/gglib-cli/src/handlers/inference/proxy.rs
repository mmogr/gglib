//! Proxy command handler.
//!
//! `gglib proxy` asks the daemon to start the unpinned proxy — the
//! counterpart to [`serve`](super::serve), which starts the same proxy
//! pinned to one model. The daemon owns the process; this command starts
//! it, then attaches the live dashboard. Ctrl-C detaches and leaves the
//! endpoint serving — `gglib proxy stop` is what stops it.

use anyhow::{Context, Result};

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, StartProxyBody};
use crate::shared_args::{AccessArgs, CacheArgs, SamplingArgs};
use gglib_core::settings::CONTEXT_SIZE_RANGE;

/// The context the daemon should serve when a client names none.
///
/// Passed through, not resolved. Resolving the chain here turned "the user set
/// nothing" into "the user set 4096" and sent it as an explicit value, so the
/// daemon's own `BuiltInDefault -> None` filter could not see that nobody had
/// chosen it, and the launch never reached the rung that fits the context to
/// the machine. `up` and `serve` already pass the setting through; this was the
/// last *serving* path that did not. The benchmark harness still resolves the
/// floor before the sweep (`benchmark/{agentic,compare,tune}`), which is
/// recorded in ADR 0009 rather than fixed here.
///
/// The flag is validated rather than silently discarded. It used to parse with
/// `.ok()`, so a typo served something else and said nothing — and with the
/// pass-through above it would have served something else again, just a
/// different something. `CtxSizeArg::parse` is deliberately *not* reused: it
/// advertises "a positive number or 'max'", and `max` has no meaning for a
/// proxy that serves every model and therefore has no single trained context in
/// scope. Pointing the user at a value this command rejects one line later is
/// worse than the silence it replaces.
fn resolve_default_context(
    flag: Option<&str>,
    settings: &gglib_core::Settings,
) -> Result<Option<u64>> {
    let Some(raw) = flag else {
        return Ok(settings.default_context_size);
    };
    let trimmed = raw.trim();
    let invalid = || {
        format!(
            "Invalid --default-context '{trimmed}'. Use a number from {} to {}; 'max' is not \
             supported here because `gglib proxy` serves every model, so no single trained \
             context is in scope. Omit the flag to fit the context to this machine.",
            CONTEXT_SIZE_RANGE.start(),
            CONTEXT_SIZE_RANGE.end(),
        )
    };
    let parsed = trimmed.parse::<u64>().with_context(invalid)?;
    // Bounded here rather than left to the daemon. This is the same value
    // `validate_settings` holds to 512..=1_000_000 and the same one
    // `--default-context-size` documents with that range, so accepting `1` on
    // this surface alone would make three descriptions of one number disagree
    // — the thing this commit is otherwise about.
    if !CONTEXT_SIZE_RANGE.contains(&parsed) {
        anyhow::bail!(invalid());
    }
    Ok(Some(parsed))
}

/// Execute the proxy command.
///
/// Ensures the daemon is running, starts the proxy on it (idempotent), and
/// attaches the dashboard until Ctrl-C.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    ctx: &CliContext,
    host: String,
    port: u16,
    default_context: Option<String>,
    sampling: SamplingArgs,
    cache: CacheArgs,
    access: AccessArgs,
) -> Result<()> {
    let settings = ctx.app.settings().get().await?;
    let default_context = resolve_default_context(default_context.as_deref(), &settings)?;

    let handle = daemon_client::ensure_daemon().await?;
    let status = handle
        .start_proxy(&StartProxyBody {
            host: Some(host.clone()),
            port: Some(port),
            default_context,
            cache: Some(cache.cache),
            slot_dir: cache.slot_dir.clone(),
            pinned: None,
            cache_disk_gb: cache.cache_disk_gb,
            inference_override: sampling.into_override(),
            // `gglib proxy` serves every model; a single default profile has no
            // model in scope to attach to. Its clients name `{model}:{profile}`.
            default_profile: None,
            api_key: access.api_key.clone(),
            allowed_hosts: access.allowed_hosts.clone(),
        })
        .await?;

    let proxy_port = status.port.unwrap_or(port);
    attach_dashboard(ctx, proxy_port, access.api_key).await
}

/// Attach the live dashboard to the running proxy, and print the detach
/// hint when the user leaves it.
///
/// Shared by `proxy`, `serve` and `up`: the daemon owns the process, so the
/// foreground command's job after starting it is to show it working.
pub(in crate::handlers) async fn attach_dashboard(
    ctx: &CliContext,
    proxy_port: u16,
    api_key_flag: Option<String>,
) -> Result<()> {
    eprintln!();
    eprintln!(
        "  Proxy running on the gglib daemon \u{2014} attaching dashboard (Ctrl-C detaches)."
    );

    // The stored key is the same row the daemon's supervisor resolves, so the
    // dashboard presents whatever the proxy demands.
    let key = match api_key_flag {
        Some(flag) => Some(flag),
        None => ctx
            .app
            .settings()
            .get()
            .await
            .ok()
            .and_then(|s| s.proxy_api_key)
            .filter(|k| !k.trim().is_empty()),
    };

    let result =
        crate::handlers::proxy_dashboard::execute("127.0.0.1".into(), proxy_port, key.as_deref())
            .await;

    eprintln!();
    eprintln!("  Detached. The proxy is still serving on port {proxy_port}.");
    eprintln!("    re-attach:  gglib proxy dashboard --port {proxy_port}");
    eprintln!("    stop it:    gglib proxy stop");
    eprintln!();
    result
}

/// Execute `gglib proxy stop`.
pub(crate) async fn stop() -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        daemon_client::DaemonProbe::Running => {}
        _ => {
            eprintln!("  Daemon is not running \u{2014} no proxy to stop.");
            return Ok(());
        }
    }
    let handle = daemon_client::DaemonHandle { client };
    let status = handle.stop_proxy().await?;
    if status.running {
        anyhow::bail!("the daemon reported the proxy still running after stop");
    }
    eprintln!("  Proxy stopped. (The daemon keeps running: `gglib daemon stop` ends it.)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_default_context;
    use gglib_core::Settings;

    /// The regression this command already shipped once.
    ///
    /// #925 made the launch fit the context to the machine, and this handler
    /// kept resolving the chain to a bare `u64` first — so the daemon was told
    /// the user had chosen 4096 and the fitted rung was never reached. Nothing
    /// was red when that happened. This is what turns red if it recurs.
    #[test]
    fn nothing_configured_sends_nothing() {
        let settings = Settings::default();
        assert_eq!(resolve_default_context(None, &settings).unwrap(), None);
    }

    #[test]
    fn a_stored_setting_is_passed_through_untouched() {
        let settings = Settings {
            default_context_size: Some(16_384),
            ..Settings::default()
        };
        assert_eq!(
            resolve_default_context(None, &settings).unwrap(),
            Some(16_384)
        );
    }

    #[test]
    fn the_flag_outranks_a_stored_setting() {
        let settings = Settings {
            default_context_size: Some(16_384),
            ..Settings::default()
        };
        assert_eq!(
            resolve_default_context(Some("8192"), &settings).unwrap(),
            Some(8192)
        );
    }

    /// It used to parse with `.ok()`, so this served 4096 and said nothing.
    #[test]
    fn a_malformed_flag_is_an_error_not_a_shrug() {
        let err = resolve_default_context(Some("8k"), &Settings::default())
            .expect_err("a value that is not a number must not be discarded");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("8k"),
            "the message must name what was rejected: {msg}"
        );
    }

    /// Zero parses as a `u64`; the help calls it invalid. The message and the
    /// behaviour have to agree.
    #[test]
    fn zero_is_rejected_because_the_message_calls_it_invalid() {
        resolve_default_context(Some("0"), &Settings::default())
            .expect_err("0 is outside the configurable range");
    }

    /// One number, three surfaces, one range. `validate_settings` rejects
    /// below 512 and `--default-context-size` documents that bound, so this
    /// flag accepting 1 would make them disagree.
    #[test]
    fn the_flag_is_held_to_the_same_range_the_settings_are() {
        use gglib_core::settings::CONTEXT_SIZE_RANGE;

        let below = CONTEXT_SIZE_RANGE.start() - 1;
        resolve_default_context(Some(&below.to_string()), &Settings::default())
            .expect_err("below the range must be rejected");

        let above = CONTEXT_SIZE_RANGE.end() + 1;
        resolve_default_context(Some(&above.to_string()), &Settings::default())
            .expect_err("above the range must be rejected");

        for edge in [*CONTEXT_SIZE_RANGE.start(), *CONTEXT_SIZE_RANGE.end()] {
            assert_eq!(
                resolve_default_context(Some(&edge.to_string()), &Settings::default()).unwrap(),
                Some(edge),
                "the range's own endpoints must be accepted"
            );
        }
    }

    /// The help says `max` is unsupported here; the error must agree with it
    /// rather than recommending it.
    #[test]
    fn max_is_rejected_and_never_recommended() {
        let err = resolve_default_context(Some("max"), &Settings::default())
            .expect_err("`max` has no meaning for a proxy serving every model");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("not supported"),
            "the message must say `max` is unsupported: {msg}"
        );
        assert!(
            !msg.contains("or 'max'"),
            "the message must not offer `max` as the remedy for rejecting `max`: {msg}"
        );
    }
}
