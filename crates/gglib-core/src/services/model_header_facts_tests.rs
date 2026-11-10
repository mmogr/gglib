//! Tests for `header_facts`: a single file, shards named by the caller or
//! found by llama.cpp's naming, and a shard that cannot be measured.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::domain::{GgufCapabilities, HeadMetadata, TensorTable};
use crate::ports::{GgufParseError, NoopGgufParser};

/// A parser that knows the tensor data of the files it is given, and
/// answers every other file as missing.
struct Payloads(HashMap<PathBuf, TensorPayload>);

impl GgufParserPort for Payloads {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        let payload = self
            .0
            .get(file_path)
            .ok_or_else(|| GgufParseError::NotFound(file_path.display().to_string()))?;
        Ok(GgufMetadata {
            payload: Some(*payload),
            ..GgufMetadata::default()
        })
    }

    fn detect_capabilities(&self, metadata: &GgufMetadata) -> GgufCapabilities {
        NoopGgufParser.detect_capabilities(metadata)
    }

    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        NoopGgufParser.tensor_table(path)
    }

    fn tensor_table_of_head(&self, head: &[u8]) -> Result<TensorTable, GgufParseError> {
        NoopGgufParser.tensor_table_of_head(head)
    }

    fn parse_head(&self, head: &[u8], file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        NoopGgufParser.parse_head(head, file_name)
    }
}

/// Tensor data of `bytes` bytes holding `elements` weights.
const fn payload(bytes: u64, elements: u64) -> TensorPayload {
    TensorPayload { bytes, elements }
}

/// The first file's parse: type 15 (`Q4_K_M`), its own tensor data, and
/// `split.count` when it is the first of a split.
fn first(own: TensorPayload, split_count: Option<u32>) -> GgufMetadata {
    let mut metadata = HashMap::new();
    if let Some(count) = split_count {
        metadata.insert("split.count".to_owned(), count.to_string());
    }
    GgufMetadata {
        file_type: Some(FileType(15)),
        payload: Some(own),
        metadata,
        ..GgufMetadata::default()
    }
}

#[test]
fn a_single_file_gives_its_own_type_and_bits() {
    let gguf = first(payload(1_000, 1_000), None);
    let facts = header_facts(Path::new("/m/a.gguf"), Some(&gguf), None, &NoopGgufParser);
    assert_eq!(facts.file_type, Some(FileType(15)));
    assert_eq!(facts.bits_per_weight, Some(8.0));
}

#[test]
fn the_shards_a_caller_names_are_summed() {
    let shards = [PathBuf::from("/m/x-1.gguf"), PathBuf::from("/m/x-2.gguf")];
    let parser = Payloads(HashMap::from([(shards[1].clone(), payload(3_000, 2_000))]));
    let gguf = first(payload(1_000, 2_000), None);
    let facts = header_facts(&shards[0], Some(&gguf), Some(&shards), &parser);
    // 4000 bytes over 4000 weights, where the first alone is 4 bits.
    assert_eq!(facts.bits_per_weight, Some(8.0));
    assert_eq!(facts.file_type, Some(FileType(15)));
}

#[test]
fn a_split_header_finds_its_shards_by_llama_cpps_naming() {
    let name = |index: u32| PathBuf::from(format!("/m/big-Q4_K_M-{index:05}-of-00003.gguf"));
    let parser = Payloads(HashMap::from([
        (name(2), payload(2_000, 1_000)),
        (name(3), payload(3_000, 1_000)),
    ]));
    let gguf = first(payload(1_000, 1_000), Some(3));
    let facts = header_facts(&name(1), Some(&gguf), None, &parser);
    assert_eq!(facts.bits_per_weight, Some(16.0));
}

#[test]
fn a_shard_that_cannot_be_read_leaves_the_bits_unknown_and_the_type_known() {
    let name = |index: u32| PathBuf::from(format!("/m/big-{index:05}-of-00002.gguf"));
    let gguf = first(payload(1_000, 1_000), Some(2));
    let facts = header_facts(&name(1), Some(&gguf), None, &Payloads(HashMap::new()));
    assert_eq!(facts.bits_per_weight, None, "half a model is not measured");
    assert_eq!(facts.file_type, Some(FileType(15)));
}

#[test]
fn a_split_whose_first_file_was_renamed_is_not_measured_in_part() {
    let gguf = first(payload(1_000, 1_000), Some(2));
    let facts = header_facts(
        Path::new("/m/renamed.gguf"),
        Some(&gguf),
        None,
        &NoopGgufParser,
    );
    assert_eq!(facts.bits_per_weight, None);
    assert_eq!(facts.file_type, Some(FileType(15)));
}

#[test]
fn no_parse_gives_no_facts() {
    let facts = header_facts(Path::new("/m/a.gguf"), None, None, &NoopGgufParser);
    assert!(facts.is_empty());
}

/// A parser that reads every file as `Q4_K_M`, its tensor data 8 bits per
/// weight.
struct TypedQ4KM;

impl GgufParserPort for TypedQ4KM {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        Ok(GgufMetadata {
            file_type: Some(FileType(15)),
            payload: Some(payload(1_000, 1_000)),
            ..GgufMetadata::default()
        })
    }

    fn detect_capabilities(&self, metadata: &GgufMetadata) -> GgufCapabilities {
        NoopGgufParser.detect_capabilities(metadata)
    }

    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        NoopGgufParser.tensor_table(path)
    }

    fn tensor_table_of_head(&self, head: &[u8]) -> Result<TensorTable, GgufParseError> {
        NoopGgufParser.tensor_table_of_head(head)
    }

    fn parse_head(&self, head: &[u8], file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        NoopGgufParser.parse_head(head, file_name)
    }
}

/// Model 1, of the file `file`, storing `file_type` and no bits per weight.
fn stored(file: &Path, file_type: Option<FileType>) -> Model {
    let mut new = crate::domain::NewModel::new(
        "zeta".to_owned(),
        file.to_path_buf(),
        Some(7.0),
        chrono::Utc::now(),
    );
    new.file_type = file_type;
    Model::stored(1, &new)
}

/// Only the missing fact is filled: a stored type stays, though the header
/// names another.
#[test]
fn a_stored_type_is_kept_when_only_the_bits_are_missing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("zeta.gguf");
    std::fs::write(&file, b"weights").unwrap();
    let mut model = stored(&file, Some(FileType(7)));

    assert!(fill_missing_facts(&mut model, &TypedQ4KM));

    assert_eq!(model.file_type, Some(FileType(7)));
    assert_eq!(model.bits_per_weight, Some(8.0));
}

/// A retag fills both facts of a model that has neither, and says so.
#[tokio::test]
async fn a_retag_fills_the_facts_a_model_lacks() {
    use crate::services::ModelService;
    use crate::services::model_projector::tests::OneModelRepo;
    use std::sync::{Arc, Mutex};

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("zeta.gguf");
    std::fs::write(&file, b"weights").unwrap();
    let repo = Arc::new(OneModelRepo(Mutex::new(stored(&file, None))));
    let service = ModelService::new(repo.clone());

    let diff = service.retag_model(1, &TypedQ4KM, false).await.unwrap();

    assert!(diff.is_some_and(|diff| diff.facts_read));
    let model = repo.0.lock().unwrap().clone();
    assert_eq!(model.file_type, Some(FileType(15)));
    assert_eq!(model.bits_per_weight, Some(8.0));
}

#[test]
fn a_single_file_counts_its_own_tensors() {
    let gguf = first(payload(1_000, 7_000_000_000), None);
    let facts = header_facts(Path::new("/m/a.gguf"), Some(&gguf), None, &NoopGgufParser);
    assert_eq!(facts.param_count_b, Some(7.0));
}

#[test]
fn every_shards_tensors_are_counted() {
    let name = |index: u32| PathBuf::from(format!("/m/big-{index:05}-of-00003.gguf"));
    let parser = Payloads(HashMap::from([
        (name(2), payload(2_000, 114_840_000_000)),
        (name(3), payload(2_000, 62_100_000_000)),
    ]));
    let gguf = first(TensorPayload::default(), Some(3));
    let facts = header_facts(&name(1), Some(&gguf), None, &parser);
    assert_eq!(facts.param_count_b, Some(176.94));
}

/// With no tensors counted, the count is what the labels or the name say.
#[test]
fn a_model_whose_tensors_cannot_all_be_counted_takes_the_labels_count() {
    let mut gguf = first(payload(1_000, 1_000), Some(2));
    gguf.param_count_b = Some(7.0);
    let facts = header_facts(
        Path::new("/m/renamed.gguf"),
        Some(&gguf),
        None,
        &NoopGgufParser,
    );
    assert_eq!(
        facts.param_count_b,
        Some(7.0),
        "half a model is not counted"
    );

    let mut unmeasured = first(payload(1_000, 1_000), None);
    unmeasured.payload = None;
    unmeasured.param_count_b = Some(13.0);
    let facts = header_facts(
        Path::new("/m/a.gguf"),
        Some(&unmeasured),
        None,
        &NoopGgufParser,
    );
    assert_eq!(facts.param_count_b, Some(13.0));
}

/// One split, its first file only metadata and its size label an `NxMB`,
/// is counted by its tensors through either door. A download names its
/// shards, here by names llama.cpp's naming would never find, so its count
/// can only come from that list. A local add finds its shards by llama.cpp's
/// naming.
#[test]
fn a_split_counts_every_shards_tensors_by_either_door() {
    use crate::services::{HfOrigin, ModelOrigin, build_new_model};

    let named: Vec<PathBuf> = ["/hub/a.gguf", "/hub/b.gguf", "/hub/c.gguf"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let local = |index: u32| PathBuf::from(format!("/m/big-{index:05}-of-00003.gguf"));
    let parser = Payloads(HashMap::from([
        (named[1].clone(), payload(2_000, 1_000_000_000)),
        (named[2].clone(), payload(2_000, 1_000_000_000)),
        (local(2), payload(2_000, 1_000_000_000)),
        (local(3), payload(2_000, 1_000_000_000)),
    ]));
    let mut gguf = first(TensorPayload::default(), Some(3));
    gguf.param_count_b = Some(28_672.0);

    let download = ModelOrigin::HuggingFace(HfOrigin {
        repo_id: "some/Big-GGUF",
        commit_sha: "abc123",
        hf_tags: &[],
        variant_label: None,
        file_paths: Some(&named),
        published_sampling: None,
    });
    let add = ModelOrigin::LocalFile {
        param_count_override: None,
    };
    for (first_file, origin) in [(named[0].clone(), download), (local(1), add)] {
        let model = build_new_model(
            &first_file,
            Some(&gguf),
            &parser,
            &origin,
            chrono::Utc::now(),
        );
        assert_eq!(model.param_count_b, Some(2.0), "{first_file:?}");
    }
}
