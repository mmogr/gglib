//! The in-process engine, for the recorder: gglib-engine-llama's shim over
//! its decoder on llama.h, asked what the pinned llama-server is asked.
//!
//! A launch here is the in-process one of `engine_contract_launch.rs`'s: the
//! same model at the same context and KV cache types, loaded in this process
//! by the decoder over llama.framework and served on loopback by the shim a
//! phone's gglib-proxy forwards to. A template the recorder names replaces
//! the model's own, as `--chat-template-file` does for llama-server, and no
//! layer goes to the GPU, as `LlamaDecoder` loads a model on a Mac by
//! default: CI's macOS runners have no GPU llama.cpp can use. The engine
//! reads no projector.
//!
//! It runs with `GGLIB_INPROC_ENGINE=1`, the models in
//! `GGLIB_ENGINE_MODELS_DIR`, and a test build that links the
//! llama.framework `GGLIB_LLAMA_XCFRAMEWORK` names: the `inproc-contract`
//! feature on a Mac (`make record-upstream-inproc`).

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V, KvCacheType};
use gglib_engine_llama::shim::{self, Shim};
use gglib_engine_llama::{
    Abort, Decoder, DecoderError, Finished, LoadParams, Loaded, Sampling, Step, Token,
    linked_llama_decoder,
};
use serde_json::Value;

use super::gemma::GREEDY;
use super::launch::{CONTEXT, Inputs as ServerInputs, Launch, STORIES, Server};
use super::project::{Engine, Recording, upstream_dir};
use super::readings::{RENDERS, TEMPLATES};

/// The name the shim serves each model under.
const NAME: &str = "contract";

/// No layer on the GPU, as `LlamaDecoder`'s default on a Mac.
const GPU_LAYERS: i32 = 0;

/// The engine a recording pass asks.
#[derive(Clone, Copy)]
pub(super) enum Upstream<'a> {
    /// The pinned llama-server, started as gglib starts it.
    LlamaServer(&'a ServerInputs),
    /// gglib-engine-llama, in this process.
    InProcess(&'a Inputs),
}

impl Upstream<'_> {
    /// Whose answers the pass records.
    pub(super) const fn engine(self) -> Engine {
        match self {
            Self::LlamaServer(_) => Engine::LlamaServer,
            Self::InProcess(_) => Engine::InProcess,
        }
    }

    /// Start `launch`, the pass's `n`th, on this engine.
    pub(super) async fn start(self, launch: &Launch<'_>, n: usize) -> Running {
        match self {
            Self::LlamaServer(inputs) => Running {
                server: Server::start(inputs, launch, n).await,
                _shim: None,
            },
            Self::InProcess(inputs) => inputs.start(launch).await,
        }
    }
}

/// A launch, answering at the [`Server`] it derefs to: a llama-server, or the
/// in-process engine's shim, which stops listening when this drops, freeing
/// the model once its last request has.
pub(super) struct Running {
    server: Server,
    _shim: Option<Shim>,
}

impl Deref for Running {
    type Target = Server;

    fn deref(&self) -> &Server {
        &self.server
    }
}

/// What the in-process recording is given: the models with `templates/`
/// beside them, and the pin llama.framework was built at.
pub(super) struct Inputs {
    models: PathBuf,
    /// The `pin.txt` `make phone-llama` writes beside the framework.
    framework_pin: String,
}

impl Inputs {
    /// `None` unless `GGLIB_INPROC_ENGINE` is `1`; the recorder then skips.
    /// With it, the models, the framework and a build that links it must be
    /// there.
    pub(super) fn from_env() -> Option<Self> {
        if std::env::var_os("GGLIB_INPROC_ENGINE").is_none_or(|flag| flag != "1") {
            return None;
        }
        let var = |name: &str| {
            let value = std::env::var_os(name).unwrap_or_else(|| {
                panic!("GGLIB_INPROC_ENGINE=1 records the in-process engine, which needs {name}")
            });
            PathBuf::from(value)
        };
        let models = var("GGLIB_ENGINE_MODELS_DIR");
        // Read from the workspace root when relative, as gglib-engine-llama's
        // build script reads it.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let xcframework = root.join(var("GGLIB_LLAMA_XCFRAMEWORK"));
        assert!(
            linked_llama_decoder(Some(0)).is_some(),
            "this build links no llama.framework: build the tests on a Mac with \
             `--features inproc-contract` and GGLIB_LLAMA_XCFRAMEWORK set, as \
             `make record-upstream-inproc` does"
        );
        let pin = xcframework.with_file_name("pin.txt");
        let framework_pin = std::fs::read_to_string(&pin).unwrap_or_else(|e| {
            panic!(
                "{}: {e}; `make phone-llama` writes it beside the framework",
                pin.display()
            )
        });
        Some(Self {
            models,
            framework_pin: framework_pin.trim().to_owned(),
        })
    }

    /// Load `launch`'s model into a fresh engine and serve it.
    async fn start(&self, launch: &Launch<'_>) -> Running {
        assert!(launch.projector.is_none(), "the engine reads no projector");
        let template = launch.template.map(|name| {
            let path = self.models.join(format!("templates/{name}.jinja"));
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        });
        let decoder = linked_llama_decoder(Some(GPU_LAYERS)).expect("from_env saw it linked");
        let engine = gglib_engine_llama::Engine::new(TemplateFile { decoder, template });
        // As the llama-server launch: f16 for stories260K's 8-wide heads,
        // gglib's default otherwise.
        let stories = launch.model == STORIES;
        let params = LoadParams {
            n_ctx: u32::try_from(CONTEXT).expect("the context fits a u32"),
            cache_type_k: if stories {
                KvCacheType::F16
            } else {
                DEFAULT_CACHE_TYPE_K
            },
            cache_type_v: if stories {
                KvCacheType::F16
            } else {
                DEFAULT_CACHE_TYPE_V
            },
        };
        let path = self.models.join(launch.model);
        let loading = Arc::clone(&engine);
        tokio::task::spawn_blocking(move || loading.load(NAME, &path, &params, &Abort::new()))
            .await
            .expect("the load ran to its end")
            .unwrap_or_else(|e| panic!("{}: {e}", launch.model));
        let shim = shim::start(engine).await.expect("the shim listens");
        Running {
            server: Server::at(&shim.base_url()),
            _shim: Some(shim),
        }
    }

    /// What the engine says of the llama.cpp it runs, from tinygemma3's
    /// `/props`, and whether it rendered each template and replied as the
    /// llama-server recording in `contracts/upstream/llama/` did.
    pub(super) fn read_own(&self, props: &Value, r: &mut Recording) {
        let build_info = props["build_info"].as_str().expect("/props build_info");
        r.texts.insert(
            "llama-inproc/version.txt".into(),
            format!(
                "build_info: {build_info}\nllama.xcframework: {}\n",
                self.framework_pin
            ),
        );
        let llama = upstream_dir().join("llama");
        for template in TEMPLATES {
            let same = RENDERS.iter().all(|(variant, _)| {
                let name = format!("apply-template.{template}.{variant}.txt");
                let theirs = std::fs::read_to_string(llama.join("bodies").join(&name)).ok();
                theirs.is_some() && r.prompts.get(&name) == theirs.as_ref()
            });
            r.plain(format!("template_parity.{template}"), same);
        }
        let observed = std::fs::read_to_string(llama.join("observed.json")).unwrap_or_default();
        let theirs: Value = serde_json::from_str(&observed).unwrap_or_default();
        let same = GREEDY.iter().all(|reading| {
            let name = format!("greedy.tinygemma3.{reading}");
            let ours = r.observed.get(&name).map(|(value, _)| value);
            ours.is_some() && ours == theirs.get(&name).map(|entry| &entry["value"])
        });
        r.plain("greedy_parity.tinygemma3", same);
    }
}

/// A decoder serving a template file in place of the model's own: the
/// in-process `--chat-template-file`.
struct TemplateFile {
    decoder: Box<dyn Decoder>,
    template: Option<String>,
}

impl Decoder for TemplateFile {
    fn load(
        &mut self,
        path: &Path,
        params: &LoadParams,
        abort: &Abort,
        progress: &mut dyn FnMut(f32),
    ) -> Result<Loaded, DecoderError> {
        let mut loaded = self.decoder.load(path, params, abort, progress)?;
        if let Some(template) = &self.template {
            loaded.chat_template.clone_from(template);
        }
        Ok(loaded)
    }

    fn tokenize(&self, text: &str) -> Result<Vec<Token>, DecoderError> {
        self.decoder.tokenize(text)
    }

    fn generate(
        &mut self,
        prompt: &[Token],
        sampling: &Sampling,
        abort: &Abort,
        sink: &mut dyn FnMut(Step),
    ) -> Result<Finished, DecoderError> {
        self.decoder.generate(prompt, sampling, abort, sink)
    }

    fn unload(&mut self) {
        self.decoder.unload();
    }
}
