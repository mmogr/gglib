//! A Hub held in memory, for a library in a test.
//!
//! It answers a search with gglib-hf's recorded search page and a lookup of
//! [`RECORDED_REPO`] with the recorded model info, whose GGUF files are that
//! repository's quantizations. Every file's head is [`chat_model`]'s. It
//! also holds files of its own, whose bytes a fake file server serves at the
//! addresses it gives, and it can refuse everything with one error.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use async_trait::async_trait;
use gglib_core::download::Quantization;
use gglib_core::ports::huggingface::HfPortResult;
use gglib_core::ports::{
    HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::test_gguf::chat_model;

/// The Hub's answer to a search for `phi-4 mini`, as gglib-hf recorded it.
const SEARCH_PAGE: &str = include_str!("../../gglib-hf/src/search_fixture.json");

/// The model info of [`RECORDED_REPO`], as gglib-hf recorded it.
const MODEL_INFO: &str = include_str!("../../gglib-hf/src/model_info_fixture.json");

/// The repository the recorded model info is of.
pub(crate) const RECORDED_REPO: &str = "unsloth/Phi-4-mini-instruct-GGUF";

/// The trained context of the model the recorded repository holds, as its
/// recorded `gguf` block says, and as each head here says too.
pub(crate) const RECORDED_TRAINED_CTX: u32 = 131_072;

/// The size this Hub gives each recorded quantization, in bytes: the
/// recorded `Q2_K` file's own size, and the others near their files' sizes
/// on the Hub. The recording holds no sizes.
pub(crate) const RECORDED_SIZES: [(&str, u64); 8] = [
    ("Q2_K", 1_682_635_744),
    ("Q2_K_L", 1_745_000_000),
    ("Q3_K_M", 2_117_000_000),
    ("Q4_K_M", 2_491_000_000),
    ("Q5_K_M", 2_848_000_000),
    ("Q6_K", 3_155_000_000),
    ("Q8_0", 4_083_000_000),
    ("BF16", 7_680_694_592),
];

/// A file this Hub holds, which its file server serves.
pub(crate) struct HubFile {
    pub(crate) repo: String,
    pub(crate) path: String,
    pub(crate) bytes: Vec<u8>,
}

impl HubFile {
    /// The quantization the file's name says it is.
    fn quantization(&self) -> String {
        Quantization::from_filename(&self.path).to_string()
    }

    /// The file's SHA-256, which the Hub gives as its LFS OID.
    pub(crate) fn digest(&self) -> String {
        Sha256::digest(&self.bytes)
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    fn info(&self) -> HfFileInfo {
        HfFileInfo {
            path: self.path.clone(),
            size: self.bytes.len() as u64,
            is_gguf: true,
            oid: Some(self.digest()),
        }
    }
}

/// A Hub held in memory.
#[derive(Default)]
pub(crate) struct TestHub {
    /// Answer every request with this error instead.
    pub(crate) refusal: Option<fn() -> HfPortError>,
    /// Files of the Hub's own, beside the recorded repository's.
    pub(crate) files: Vec<HubFile>,
    /// The address of the server that serves [`Self::files`].
    pub(crate) file_server: Option<String>,
}

impl TestHub {
    fn refused(&self) -> HfPortResult<()> {
        self.refusal.map_or(Ok(()), |refusal| Err(refusal()))
    }

    /// The recorded repository's GGUF files, by quantization.
    fn recorded_quantizations() -> BTreeMap<String, Vec<String>> {
        let info: Value = serde_json::from_str(MODEL_INFO).expect("the recording is JSON");
        let mut quants: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for sibling in info["siblings"]
            .as_array()
            .expect("the recording lists files")
        {
            let path = sibling["rfilename"].as_str().expect("a file has a name");
            if std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "gguf")
            {
                let name = Quantization::from_filename(path).to_string();
                quants.entry(name).or_default().push(path.to_owned());
            }
        }
        quants
    }

    fn files_of<'a>(&'a self, repo: &'a str) -> impl Iterator<Item = &'a HubFile> {
        self.files.iter().filter(move |file| file.repo == repo)
    }
}

/// A recorded repository's summary, read as gglib-hf reads it.
fn summary(recorded: &Value) -> HfRepoInfo {
    let model_id = recorded["id"]
        .as_str()
        .expect("a recording names its repository");
    HfRepoInfo {
        model_id: model_id.to_owned(),
        name: model_id.rsplit('/').next().unwrap_or(model_id).to_owned(),
        author: model_id.split('/').next().map(str::to_owned),
        downloads: recorded["downloads"].as_u64().unwrap_or(0),
        likes: recorded["likes"].as_u64().unwrap_or(0),
        parameters_b: recorded["gguf"]["total"].as_f64().map(|total| total / 1e9),
        description: None,
        last_modified: None,
        chat_template: None,
        architecture: recorded["gguf"]["architecture"].as_str().map(str::to_owned),
        context_length: recorded["gguf"]["context_length"].as_u64(),
        tags: Vec::new(),
    }
}

#[async_trait]
impl HfClientPort for TestHub {
    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        self.refused()?;
        let page: Vec<Value> = serde_json::from_str(SEARCH_PAGE).expect("the recording is JSON");
        Ok(HfSearchResult {
            items: page.iter().map(summary).collect(),
            next_cursor: Some("next".to_owned()),
        })
    }

    async fn list_quantizations(&self, model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        self.refused()?;
        if model_id == RECORDED_REPO {
            return Ok(Self::recorded_quantizations()
                .into_iter()
                .map(|(name, file_paths)| HfQuantInfo {
                    total_size: RECORDED_SIZES
                        .iter()
                        .find(|(quant, _)| *quant == name)
                        .map_or(0, |(_, size)| *size),
                    shard_count: file_paths.len(),
                    file_paths,
                    name,
                })
                .collect());
        }
        Ok(self
            .files_of(model_id)
            .map(|file| HfQuantInfo {
                name: file.quantization(),
                shard_count: 1,
                total_size: file.bytes.len() as u64,
                file_paths: vec![file.path.clone()],
            })
            .collect())
    }

    async fn list_projectors(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        self.refused()?;
        Ok(Vec::new())
    }

    async fn list_gguf_files(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        self.refused()?;
        Ok(self.files_of(model_id).map(HubFile::info).collect())
    }

    async fn get_quantization_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        self.refused()?;
        let files: Vec<HfFileInfo> = self
            .files_of(model_id)
            .filter(|file| file.quantization() == quantization)
            .map(HubFile::info)
            .collect();
        if files.is_empty() {
            return Err(HfPortError::QuantizationNotFound {
                model_id: model_id.to_owned(),
                quantization: quantization.to_owned(),
            });
        }
        Ok(files)
    }

    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        self.refused()?;
        Ok("0".repeat(40))
    }

    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo> {
        self.refused()?;
        if model_id == RECORDED_REPO {
            let info: Value = serde_json::from_str(MODEL_INFO).expect("the recording is JSON");
            return Ok(summary(&info));
        }
        Err(HfPortError::ModelNotFound {
            model_id: model_id.to_owned(),
        })
    }

    /// The head of one of the Hub's own files, or else [`chat_model`]'s.
    async fn read_head(&self, model_id: &str, path: &str, max_bytes: u64) -> HfPortResult<Vec<u8>> {
        self.refused()?;
        let whole = self
            .files_of(model_id)
            .find(|file| file.path == path)
            .map_or_else(
                || chat_model("phi3", RECORDED_TRAINED_CTX).bytes(),
                |file| file.bytes.clone(),
            );
        let cap = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        Ok(whole[..whole.len().min(cap)].to_vec())
    }

    fn file_url(&self, model_id: &str, path: &str, revision: &str) -> String {
        let server = self
            .file_server
            .as_deref()
            .unwrap_or("http://no-server.invalid");
        format!("{server}/{model_id}/resolve/{revision}/{path}")
    }

    async fn file_at(&self, model_id: &str, path: &str) -> HfPortResult<Option<HfFileInfo>> {
        self.refused()?;
        Ok(self
            .files_of(model_id)
            .find(|file| file.path == path)
            .map(HubFile::info))
    }
}
