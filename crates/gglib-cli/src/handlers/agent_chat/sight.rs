//! Whether the model a session talks to can read an image, asked before
//! the agent loop is composed and before any model is loaded.
//!
//! The rule, the code and the words are core's
//! ([`gglib_core::request_pipeline::refuse_unless_can_see`]); this is where
//! the CLI finds out what to ask it with. A model of this machine is judged
//! by its catalogue row: it can see when a projector is linked. A session
//! on `--port` talks to the server on that port, so that server is asked
//! first, through its `/props`; the row decides only when the server does
//! not say, and a row with a projector on a server that cannot see is told
//! the server was started without it. A model of the paired machine is not
//! judged here: its own proxy refuses the request by the same code.

use std::time::Duration;

use anyhow::{Result, anyhow};
use gglib_core::request_pipeline::refuse_unless_can_see;
use serde_json::Value;

use super::config::AgentSessionParams;
use crate::bootstrap::CliContext;
use crate::target::Target;

/// How long a server named with `--port` is given to answer `/props`.
const PROPS_TIMEOUT: Duration = Duration::from_secs(2);

/// What a session knows of its model's image input.
pub(crate) struct Sight {
    /// The name the refusal calls the model by.
    model: String,
    source: Source,
}

/// Where the answer comes from.
enum Source {
    /// The catalogue row's `image_input`.
    Catalogue(bool),
    /// The llama-server on this loopback port, asked when an image is sent,
    /// and the catalogue row's `image_input` under the session's model, for
    /// when the server does not say.
    Server {
        client: reqwest::Client,
        port: u16,
        catalogue: Option<bool>,
    },
    /// Nothing here can say; whatever serves the turn answers.
    Unjudged,
}

impl Sight {
    /// A session nothing here can judge.
    pub(crate) const fn unjudged() -> Self {
        Self {
            model: String::new(),
            source: Source::Unjudged,
        }
    }

    /// A catalogue model called `model`, which reads images or does not.
    pub(crate) fn catalogue(model: &str, image_input: bool) -> Self {
        Self {
            model: model.to_owned(),
            source: Source::Catalogue(image_input),
        }
    }

    /// The llama-server on `port`, called `model` in a refusal; `catalogue`
    /// is the row's `image_input`, `None` when there is no row to read.
    pub(crate) fn server(
        model: &str,
        client: reqwest::Client,
        port: u16,
        catalogue: Option<bool>,
    ) -> Self {
        Self {
            model: model.to_owned(),
            source: Source::Server {
                client,
                port,
                catalogue,
            },
        }
    }

    /// What the session `params` describes knows of its model.
    ///
    /// `--port` names the server that answers, so there the server is the
    /// one asked, and the row is read only for when it does not say. A row
    /// that cannot be read there is no row: the server still judges.
    ///
    /// # Errors
    ///
    /// A catalogue that cannot be read: a model it could not look up is not
    /// one it has no row for, and is not left unjudged.
    pub(crate) async fn of_session(ctx: &CliContext, params: &AgentSessionParams) -> Result<Self> {
        let name = params
            .turn
            .as_ref()
            .map_or(params.model_identifier.as_str(), |turn| turn.name.as_str());
        Ok(match (params.target, params.port) {
            (Target::Remote, _) => Self::unjudged(),
            (Target::Local, Some(port)) => {
                let catalogue = params
                    .target
                    .local_model(ctx, &params.model_identifier)
                    .await
                    .ok()
                    .flatten()
                    .map(|model| model.image_input());
                Self::server(name, ctx.http_client.clone(), port, catalogue)
            }
            (Target::Local, None) => params
                .target
                .local_model(ctx, &params.model_identifier)
                .await?
                .map_or_else(Self::unjudged, |model| {
                    Self::catalogue(&model.name, model.image_input())
                }),
        })
    }

    /// Refuse a run that carries an image when the model cannot read one.
    /// A run with no image asks nothing of anyone.
    ///
    /// # Errors
    ///
    /// Core's refusal, naming the model and the command that links a
    /// projector, or, on a server that cannot see a model whose row has
    /// one, saying that server was started without it.
    pub(crate) async fn admit(&self, has_images: bool) -> Result<()> {
        if !has_images {
            return Ok(());
        }
        let (image_input, linked_on) = match &self.source {
            Source::Catalogue(image_input) => (*image_input, None),
            Source::Server {
                client,
                port,
                catalogue,
            } => {
                let served = server_sees(client, *port).await;
                let linked_on = (*catalogue == Some(true)).then_some(*port);
                (served.or(*catalogue).unwrap_or(true), linked_on)
            }
            Source::Unjudged => (true, None),
        };
        refuse_unless_can_see(image_input, has_images).map_err(|refusal| {
            anyhow!(linked_on.map_or_else(
                || refusal.message(&self.model),
                |port| refusal.started_without_projector(&self.model, port),
            ))
        })
    }
}

/// What the llama-server on `port` says of its image input: its `/props`
/// `modalities.vision`. `None` when it does not answer, or answers without
/// one (an older server has no such field), and the row decides.
async fn server_sees(client: &reqwest::Client, port: u16) -> Option<bool> {
    let response = client
        .get(format!("http://127.0.0.1:{port}/props"))
        .timeout(PROPS_TIMEOUT)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let props: Value = response.json().await.ok()?;
    props.get("modalities")?.get("vision")?.as_bool()
}

#[cfg(test)]
#[path = "sight_tests.rs"]
pub(crate) mod sight_tests;
