#![doc = include_str!("README.md")]

mod chat;
mod frames;
mod info;
mod reasoning;
mod reply;
mod stop;

#[cfg(test)]
#[path = "refusal_tests.rs"]
mod refusal_tests;
#[cfg(test)]
#[path = "shape_tests.rs"]
mod shape_tests;
#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;
#[cfg(test)]
#[path = "template_tests.rs"]
mod template_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
#[path = "thinking_tests.rs"]
mod thinking_tests;

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use gglib_core::contracts::http::MAX_BODY_BYTES;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::engine::Engine;

/// The shim, listening on a port of loopback.
///
/// Dropping it stops the listener too, but without aborting the generation
/// in flight or waiting for anything: [`Shim::shutdown`] does both.
#[derive(Debug)]
pub struct Shim {
    addr: SocketAddr,
    engine: Arc<Engine>,
    stop: oneshot::Sender<()>,
    served: JoinHandle<io::Result<()>>,
}

impl Shim {
    /// The port it listens on, on 127.0.0.1.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// `http://127.0.0.1:<port>`, the base its endpoints hang off.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Abort the generation in flight, stop listening, and wait for the
    /// connections still open to finish.
    ///
    /// A request still waiting its turn runs once its turn comes, unless
    /// the engine's model is unloaded first, which answers it with a 503.
    pub async fn shutdown(self) -> io::Result<()> {
        self.engine.abort();
        // The server may already have stopped, with nothing left to tell.
        let _ = self.stop.send(());
        self.served
            .await
            .map_err(|e| io::Error::other(format!("the shim's server task failed: {e}")))?
    }
}

/// Start the shim for `engine` on a free port of 127.0.0.1, on the current
/// tokio runtime.
pub async fn start(engine: Arc<Engine>) -> io::Result<Shim> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel::<()>();
    let app = router(Arc::clone(&engine));
    let served = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                // A dropped sender stops the server as a sent one does.
                let _ = stopped.await;
            })
            .await
    });
    Ok(Shim {
        addr,
        engine,
        stop,
        served,
    })
}

/// llama-server's endpoints that gglib reads of an upstream: the three
/// gglib-proxy asks, the two its contract recorder reads, and the slot
/// actions the proxy's slot persistence would ask, refused as llama-server
/// started without `--slot-save-path` refuses them. Anything else is
/// llama-server's 404.
fn router(engine: Arc<Engine>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(chat::chat_completions))
        .route("/apply-template", post(info::apply_template))
        .route("/props", get(info::props))
        .route("/health", get(info::health))
        .route("/slots", get(info::slots))
        .route("/slots/{id_slot}", post(info::slot_action))
        .fallback(info::not_found)
        .method_not_allowed_fallback(info::not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(engine)
}
