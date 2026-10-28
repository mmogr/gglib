//! A runtime over a [`ScriptedDecoder`] and a catalog of launch specs, on a
//! device with a budget the test chooses, and the models' weights on disk.

use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::domain::device_budget::DeviceReading;
use gglib_core::domain::{KvElemsPerToken, ModelSamplingDefaults};
use gglib_core::ports::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};
use tempfile::TempDir;
use tokio::sync::watch;

use super::InProcessRuntime;
use crate::decoder::{Script, ScriptRecord, ScriptedDecoder};
use crate::state::EngineState;

pub(super) const GIB: u64 = 1024 * 1024 * 1024;

/// The budget the tests' device gives a model: 2 GiB, the memory still
/// available, which is under its share of 8 GiB.
pub(super) const BUDGET: u64 = 2 * GIB;

/// K and V elements per token of the tests' models: at `Q8_0`, 69,632 bytes
/// of cache a token.
const KV: KvElemsPerToken = KvElemsPerToken {
    k: 32_768,
    v: 32_768,
};

/// A device whose budget is [`BUDGET`].
pub(super) const fn device() -> DeviceReading {
    DeviceReading {
        physical_bytes: 8 * GIB,
        available_bytes: BUDGET,
        is_simulator: false,
    }
}

/// A chat model `name`, catalog id `id`, of `weights` bytes, trained to
/// 32,768 tokens, its weights nowhere on disk: [`Shelf::model`] puts them
/// there.
///
/// At 1 GiB of weights it fits [`BUDGET`] at 8192 tokens: 90% of the budget
/// less the weights leaves room for 12,336 tokens of cache, and 8192 is the
/// rung below that.
pub(super) fn model(id: u32, name: &str, weights: u64) -> ModelLaunchSpec {
    ModelLaunchSpec {
        id,
        name: name.to_owned(),
        file_path: PathBuf::from(format!("/models/{name}.gguf")),
        projector: None,
        image_family: None,
        components: Vec::new(),
        tags: Vec::new(),
        architecture: Some("llama".to_owned()),
        quantization: Some("Q4_K_M".to_owned()),
        context_length: Some(32_768),
        server_defaults: None,
        file_size_bytes: weights,
        kv_elems_per_token: Some(KV),
        kv_memory_is_partial: false,
        model_sampling: ModelSamplingDefaults::default(),
    }
}

/// A folder of model files for one test. Each file is as long as it says
/// and sparse, so a model of gigabytes takes no disk; the fit reads the
/// weights' size from here, as it reads a downloaded model's.
pub(super) struct Shelf(TempDir);

impl Shelf {
    /// An empty shelf, whose folder goes when it is dropped.
    pub(super) fn new() -> Self {
        Self(tempfile::tempdir().expect("a folder for the models"))
    }

    /// A file `name` of `bytes` bytes on the shelf.
    pub(super) fn file(&self, name: &str, bytes: u64) -> PathBuf {
        let path = self.0.path().join(name);
        File::create(&path)
            .and_then(|file| file.set_len(bytes))
            .expect("the file is written");
        path
    }

    /// [`model`], its weights a file on the shelf.
    pub(super) fn model(&self, id: u32, name: &str, weights: u64) -> ModelLaunchSpec {
        let mut spec = model(id, name, weights);
        spec.file_path = self.file(&format!("{name}.gguf"), weights);
        spec
    }
}

/// A catalog of launch specs, found by id or by name as the real one finds
/// them. It lists nothing: the runtime only resolves for launch.
#[derive(Debug)]
pub(super) struct Catalog(pub(super) Vec<ModelLaunchSpec>);

#[async_trait]
impl ModelCatalogPort for Catalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(Vec::new())
    }

    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }

    async fn resolve_for_launch(
        &self,
        name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        let by_id = name.parse::<u32>().ok();
        Ok(self
            .0
            .iter()
            .find(|m| Some(m.id) == by_id || m.name == name)
            .cloned())
    }
}

/// A runtime over `script` on [`device`], its catalog `models`, and what
/// its decoder recorded.
pub(super) async fn runtime(
    script: Script,
    models: Vec<ModelLaunchSpec>,
) -> (Arc<InProcessRuntime>, Arc<Mutex<ScriptRecord>>) {
    let (decoder, record) = ScriptedDecoder::new(script);
    let runtime = InProcessRuntime::start(decoder, Arc::new(Catalog(models)), device())
        .await
        .expect("the runtime starts");
    (Arc::new(runtime), record)
}

/// A script that answers "Hi." in two pieces.
pub(super) fn hello() -> Script {
    Script::new(["Hi", "."])
}

/// [`hello`], each step taking `step` and a load `load`.
pub(super) fn slow(step: Duration, load: Duration) -> Script {
    let mut script = hello();
    script.step_delay = step;
    script.load_delay = load;
    script
}

/// What `record` holds now.
pub(super) fn read(record: &Mutex<ScriptRecord>) -> ScriptRecord {
    record
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Wait, at most five seconds, for `rx` to show a state `want` accepts.
pub(super) async fn until(
    rx: &mut watch::Receiver<EngineState>,
    want: impl Fn(&EngineState) -> bool + Send + Sync,
) -> EngineState {
    let shown = rx.wait_for(|state| want(state));
    let seen = tokio::time::timeout(Duration::from_secs(5), shown)
        .await
        .expect("the state came within five seconds")
        .expect("the watch had not ended");
    seen.clone()
}
