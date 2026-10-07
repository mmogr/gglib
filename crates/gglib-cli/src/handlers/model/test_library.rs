//! A library in a temporary directory, and the pieces a test of a `gglib
//! model` command drives it with: real GGUF headers, the command line as
//! clap parses it, and a `ModelOps` whose events and runtime the test holds.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    stopped: AtomicBool,
}

impl Runtime {
    /// A runtime with model `id` being served on `port`.
    pub(super) fn serving(id: i64, port: u16) -> Self {
        Self {
            serving: Some((id, port)),
            stopped: AtomicBool::new(false),
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
