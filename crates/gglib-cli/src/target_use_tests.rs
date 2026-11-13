//! Tests for [`super`]: where `proxy dashboard` and `proxy cache-clear`
//! connect — the port flag, the stored `proxy_port` behind it, and the
//! paired machine's proxy under `--remote`.

use clap::Parser as _;
use gglib_core::Settings;
use gglib_core::settings::DEFAULT_PROXY_PORT;

use super::*;
use crate::bootstrap::test_context;
use crate::commands::{Commands, ProxyCommand};
use crate::daemon_client::STAND_IN_PORT;
use crate::parser::Cli;

/// The two commands that connect to a proxy rather than start one.
const CLIENTS: [&str; 2] = ["dashboard", "cache-clear"];

/// The host, port and key flags of `gglib proxy <client> <extra…>`, as
/// dispatch hands them to [`Target::proxy_endpoint`].
fn flags(client: &str, extra: &[&str]) -> (String, Option<u16>, Option<String>) {
    let argv: Vec<&str> = ["gglib", "proxy", client]
        .into_iter()
        .chain(extra.iter().copied())
        .collect();
    match Cli::parse_from(&argv).command {
        Some(Commands::Proxy {
            command:
                Some(
                    ProxyCommand::Dashboard {
                        host,
                        port,
                        api_key,
                        ..
                    }
                    | ProxyCommand::CacheClear {
                        host,
                        port,
                        api_key,
                        ..
                    },
                ),
            ..
        }) => (host, port, api_key),
        _ => panic!("{argv:?} is not a proxy client command"),
    }
}

/// The CLI's context over `dir`'s database, with `proxy_port` stored as
/// `stored`.
async fn context(dir: &tempfile::TempDir, stored: Option<u16>) -> CliContext {
    let ctx = test_context(dir.path()).await;
    ctx.settings_repo
        .modify(&|settings: &mut Settings| {
            settings.proxy_port = stored;
            Ok(())
        })
        .await
        .expect("the port is stored");
    ctx
}

/// The port `gglib proxy <client> <extra…>` connects to on this machine.
async fn local_port(ctx: &CliContext, client: &str, extra: &[&str]) -> u16 {
    let (host, port, api_key) = flags(client, extra);
    let (_, port, _) = Target::Local
        .proxy_endpoint(ctx, host, port, api_key)
        .await
        .expect("an endpoint on this machine");
    port
}

/// With no `--port`, both commands connect where the stored `proxy_port`
/// says the proxy is: the port the desktop app, the tray and `gglib proxy`
/// start it on.
#[tokio::test]
async fn without_a_port_flag_a_proxy_client_connects_to_the_stored_port() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Some(9000)).await;

    for client in CLIENTS {
        assert_eq!(local_port(&ctx, client, &[]).await, 9000, "{client}");
    }
}

#[tokio::test]
async fn a_port_flag_outranks_the_stored_port() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Some(9000)).await;

    for client in CLIENTS {
        let port = local_port(&ctx, client, &["--port", "8123"]).await;
        assert_eq!(port, 8123, "{client}");
    }
}

/// With nothing stored and no flag, the port is the one default.
#[tokio::test]
async fn with_nothing_stored_a_proxy_client_connects_to_the_default_port() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, None).await;

    for client in CLIENTS {
        let port = local_port(&ctx, client, &[]).await;
        assert_eq!(port, DEFAULT_PROXY_PORT, "{client}");
    }
}

/// With no flag typed there is nothing to refuse (the parse refuses a typed
/// one, `remote_flag_tests`), and a key from `GGLIB_API_KEY` is not one: the
/// command goes on to the paired machine, and stops here only because no
/// daemon answers.
#[tokio::test]
async fn remote_with_no_flag_typed_is_not_refused_for_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Some(9000)).await;
    // A port nothing listens on, standing in for the daemon's, so the probe
    // never reaches a daemon running on this machine.
    let nobody = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a loopback port")
        .port();

    for client in CLIENTS {
        let (host, port, _) = flags(client, &[]);
        let err = STAND_IN_PORT
            .scope(
                nobody,
                Target::Remote.proxy_endpoint(&ctx, host, port, Some("from-env".to_owned())),
            )
            .await
            .expect_err("no daemon answers");
        let text = err.to_string();
        assert!(!text.contains("drop"), "{client}: {text}");
        assert!(text.contains("gglib remote join"), "{client}: {text}");
    }
}
