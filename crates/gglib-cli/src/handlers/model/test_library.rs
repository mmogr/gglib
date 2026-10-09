//! A library in a temporary directory, and the pieces a test of a `gglib
//! model` command drives it with: real GGUF headers, the command line as
//! clap parses it, and a `ModelOps` whose events and runtime the test holds.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Result;
use gglib_app_services::{ModelDeps, ModelOps};
use gglib_core::Model;
use gglib_core::events::AppEvent;
use gglib_core::ports::{
    Admission, AppEventEmitter, LaunchOverrides, ModelRuntimeError, ModelRuntimePort,
    ProcessHandle, RunningTarget,
};
use gglib_core::services::ImportMode;

use crate::bootstrap::{CliContext, test_context};

/// A GGUF v3 file in `dir` holding only `pairs` as string metadata, under its
/// canonical path.
pub(super) fn write_gguf(dir: &Path, name: &str, pairs: &[(&str, &str)]) -> PathBuf {
    let path = dir.join(name);
    gglib_gguf::write_string_gguf(&path, pairs);
    path.canonicalize().unwrap()
}

/// A library in `dir` holding one model, and that model. Whatever a command
/// resolves from the data root is this test binary's own, not the checkout's.
pub(super) async fn library(dir: &Path) -> (CliContext, Model) {
    gglib_core::paths::isolate_data_root();
    let ctx = test_context(dir).await;
    let weights = write_gguf(dir, "qwen.Q8_0.gguf", &[("general.architecture", "qwen3")]);
    let model = ctx
        .app
        .models()
        .import_from_file(&weights, ctx.gguf_parser.as_ref(), None, ImportMode::Fresh)
        .await
        .expect("the model imports");
    (ctx, model)
}

/// The tensors of a Flux.1 diffusion model's GGUF that its family is read
/// from, shapes outermost first.
pub(super) const FLUX_TENSORS: &[(&str, &[u64])] = &[
    ("double_blocks.0.img_attn.qkv.weight", &[9216, 3072]),
    ("single_blocks.0.linear1.weight", &[21504, 3072]),
    ("img_in.weight", &[3072, 64]),
    ("txt_in.weight", &[3072, 4096]),
];

/// The tensors each of Flux.1's components is checked by: the VAE's, the
/// CLIP-L's and the T5-XXL's.
pub(super) const FLUX_VAE: &[(&str, &[u64])] = &[
    ("decoder.conv_in.weight", &[512, 16, 3, 3]),
    ("encoder.conv_in.weight", &[128, 3, 3, 3]),
];
pub(super) const FLUX_CLIP_L: &[(&str, &[u64])] = &[
    (
        "text_model.embeddings.token_embedding.weight",
        &[49408, 768],
    ),
    ("text_model.encoder.layers.11.mlp.fc1.weight", &[3072, 768]),
];
pub(super) const FLUX_T5XXL: &[(&str, &[u64])] = &[
    ("shared.weight", &[32128, 4096]),
    (
        "encoder.block.23.layer.0.SelfAttention.q.weight",
        &[4096, 4096],
    ),
];

/// A Flux.1 diffusion model's GGUF in `dir`, with no metadata as the real
/// ones have none, under its canonical path.
pub(super) fn write_flux(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    gglib_gguf::write_tensor_gguf(&path, &[], FLUX_TENSORS);
    path.canonicalize().unwrap()
}

/// A safetensors file in `dir` holding `tensors`, under its canonical path.
pub(super) fn write_component(dir: &Path, name: &str, tensors: &[(&str, &[u64])]) -> PathBuf {
    let path = dir.join(name);
    gglib_gguf::write_safetensors(&path, tensors);
    path.canonicalize().unwrap()
}

/// A library in `dir` holding one Flux.1 model, imported from its file, and
/// that model.
pub(super) async fn image_library(dir: &Path) -> (CliContext, Model) {
    gglib_core::paths::isolate_data_root();
    let ctx = test_context(dir).await;
    let weights = write_flux(dir, "flux1-schnell-q8_0.gguf");
    let model = ctx
        .app
        .models()
        .import_from_file(
            &weights,
            ctx.gguf_parser.as_ref(),
            Some(12.0),
            ImportMode::Fresh,
        )
        .await
        .expect("the image model imports");
    (ctx, model)
}

/// The row stored under `id`, or `None` once it is gone.
pub(super) async fn row(ctx: &CliContext, id: i64) -> Option<Model> {
    ctx.app.models().get_by_id(id).await.expect("the row reads")
}

pub(super) async fn stored(ctx: &CliContext, id: i64) -> Model {
    row(ctx, id).await.expect("stored")
}

/// `argv` parsed as the CLI parses it.
fn parsed(argv: &[&str]) -> Result<crate::ModelCommand> {
    use clap::Parser as _;
    let cli = crate::Cli::try_parse_from(argv)?;
    let Some(crate::Commands::Model { command }) = cli.command else {
        panic!("{argv:?} is not a model command");
    };
    Ok(command)
}

/// `argv` parsed as the CLI parses it and run as `gglib model …` runs it.
pub(super) async fn run(ctx: &CliContext, argv: &[&str]) -> Result<()> {
    super::dispatch(ctx, parsed(argv)?, crate::target::Target::Local).await
}

/// [`run`], through `ops` in place of the ones a command builds for itself.
pub(super) async fn run_with(ctx: &CliContext, ops: &ModelOps, argv: &[&str]) -> Result<()> {
    super::dispatch_with(ctx, ops, parsed(argv)?, crate::target::Target::Local).await
}

/// An emitter that keeps what it was told, in the order it was told.
#[derive(Default)]
pub(super) struct Heard(Mutex<Vec<AppEvent>>);

impl Heard {
    /// Everything emitted so far, oldest first.
    pub(super) fn events(&self) -> Vec<AppEvent> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl AppEventEmitter for Heard {
    fn emit(&self, event: AppEvent) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}

/// A runtime serving `serving`, a model id and its port, or nothing; it
/// keeps whether it was told to stop.
#[derive(Debug, Default)]
pub(super) struct Runtime {
    pub(super) serving: Option<(i64, u16)>,
    /// How many times it is asked what is running before `serving` shows.
    quiet_for: usize,
    asked: AtomicUsize,
    stopped: AtomicBool,
}

impl Runtime {
    /// A runtime with model `id` being served on `port`.
    pub(super) fn serving(id: i64, port: u16) -> Self {
        Self::serving_after(0, id, port)
    }

    /// A runtime that answers "nothing" the first `asks` times it is asked
    /// what is running, and from then on has model `id` served on `port`: a
    /// server that came up in between.
    pub(super) fn serving_after(asks: usize, id: i64, port: u16) -> Self {
        Self {
            serving: Some((id, port)),
            quiet_for: asks,
            ..Self::default()
        }
    }

    /// Whether anything asked it to stop what it serves.
    pub(super) fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ModelRuntimePort for Runtime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        unimplemented!("an update and a removal start nothing")
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn list_running(&self) -> Vec<ProcessHandle> {
        if self.asked.fetch_add(1, Ordering::SeqCst) < self.quiet_for {
            return Vec::new();
        }
        self.serving
            .iter()
            .map(|&(id, port)| ProcessHandle::new(id, "served".to_owned(), None, port, 0))
            .collect()
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// `ModelOps` over `ctx`'s library that tells `heard` what it changes and
/// asks `runtime` what is being served.
pub(super) fn ops(ctx: &CliContext, heard: &Arc<Heard>, runtime: &Arc<Runtime>) -> ModelOps {
    ModelOps::new(ModelDeps {
        core: ctx.app.clone(),
        runtime: Arc::clone(runtime) as Arc<dyn ModelRuntimePort>,
        gguf_parser: ctx.gguf_parser.clone(),
        emitter: Arc::clone(heard) as Arc<dyn AppEventEmitter>,
    })
}
