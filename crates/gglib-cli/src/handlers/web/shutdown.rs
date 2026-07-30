//! Shutdown-signal handling for `gglib web`.
//!
//! `axum::serve` is awaited to completion by `gglib_axum::start_server`, so the
//! web handler has no shutdown hook of its own. Racing the server against this
//! future gives the handler a chance to run its own teardown — withdrawing the
//! mDNS record — instead of the process simply dying with the record still
//! advertised.

/// Resolve when the process is asked to stop.
///
/// Completes on SIGINT (Ctrl-C) on every platform, and additionally on SIGTERM
/// on Unix, which is what a service manager sends.
pub async fn shutdown_signal() {
    let interrupt = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!("could not listen for Ctrl-C ({e}); shutdown will not be graceful");
            // Never resolve: let the other branch (or the server) decide.
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut sig) => {
                    sig.recv().await;
                }
                Err(e) => {
                    tracing::warn!("could not listen for SIGTERM ({e})");
                    std::future::pending::<()>().await;
                }
            }
        };

        tokio::select! {
            () = interrupt => {}
            () = terminate => {}
        }
    }

    #[cfg(not(unix))]
    interrupt.await;
}
