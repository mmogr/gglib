//! The summary's parameter count: the Hub's while the repository's files
//! could hold it and their names agree, else their names', else their
//! header's.

use std::path::Path;

use gglib_core::domain::TensorTable;
use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::fake_hub::FakeHub;
use gglib_core::ports::huggingface::variants_of;
use gglib_core::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, HeadMetadata, NoopGgufParser,
};

use super::*;

const REPO: &str = "HauhauCS/Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF";
const Q8: &str = "Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf";

/// Reads a head as the count its bytes spell, in billions, of a model's
/// weights; a head of `draft` is a sidecar's of 1.9B.
struct CountParser;

impl GgufParserPort for CountParser {
    fn parse(&self, path: &Path) -> Result<GgufMetadata, GgufParseError> {
        NoopGgufParser.parse(path)
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
    fn parse_head(&self, head: &[u8], _file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        let (role, count) = match head {
            b"draft" => (GgufFileRole::Sidecar, Some(1.9)),
            spelt => (
                GgufFileRole::Weights,
                std::str::from_utf8(spelt).ok().and_then(|s| s.parse().ok()),
            ),
        };
        Ok(HeadMetadata {
            role: Some(role),
            metadata: GgufMetadata {
                param_count_b: count,
                ..GgufMetadata::default()
            },
            ..HeadMetadata::default()
        })
    }
}

/// `HauhauCS`'s variants at their sizes on the Hub: its largest weights, its
/// `FastMTP` sidecar and its projector.
fn hauhaucs() -> Vec<HfQuantInfo> {
    variants_of([
        (Q8, 31_457_990_784),
        (
            "Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-IQ2_M.gguf",
            10_319_906_944,
        ),
        (
            "Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-FastMTP-32K.gguf",
            903_453_952,
        ),
        (
            "mmproj-Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-BF16.gguf",
            931_146_624,
        ),
    ])
}

/// A repository whose model files' names state no size, at `HauhauCS`'s
/// sizes.
fn unnamed() -> Vec<HfQuantInfo> {
    variants_of([
        (UNNAMED_Q8, 31_457_990_784),
        ("model-IQ2_M.gguf", 10_319_906_944),
        ("mmproj-F16.gguf", 931_146_624),
    ])
}

const UNNAMED_Q8: &str = "model-Q8_K_P.gguf";

fn hub_with_head(path: &str, head: &[u8]) -> FakeHub {
    FakeHub {
        heads: vec![(path.to_owned(), head.to_vec())],
        ..FakeHub::default()
    }
}

/// The Hub's 1.9B, its draft's count, is not `HauhauCS`'s: the 27B its
/// files' names state is, which its largest weights hold, with no head read.
#[tokio::test]
async fn a_count_the_hub_read_from_a_draft_gives_way_to_the_names() {
    let hub = hub_with_head(Q8, b"27");

    let counted =
        checked_parameters(&hub, &CountParser, REPO, Some(1.863_907_84), &hauhaucs()).await;

    assert_eq!(counted, Some(27.0));
    assert!(hub.heads_asked.lock().unwrap().is_empty());
}

/// With names that state no size, a rejected Hub count gives way to the
/// head of the largest weights, while those weights could hold it.
#[tokio::test]
async fn names_that_state_nothing_give_way_to_the_models_header() {
    let hub = hub_with_head(UNNAMED_Q8, b"27");

    let counted =
        checked_parameters(&hub, &CountParser, REPO, Some(1.863_907_84), &unnamed()).await;

    assert_eq!(counted, Some(27.0));
    let asked = hub.heads_asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].1, UNNAMED_Q8);
}

/// A count the files can hold, which the names do not contradict, is the
/// Hub's, and no head is read for it.
#[tokio::test]
async fn a_count_the_files_can_hold_is_the_hubs() {
    let hub = hub_with_head(Q8, b"27");

    let counted = checked_parameters(&hub, &CountParser, REPO, Some(27.3), &hauhaucs()).await;

    assert_eq!(counted, Some(27.3));
    assert!(hub.heads_asked.lock().unwrap().is_empty());
}

/// With the Hub's count rejected and no name to go by, a head that says it
/// is no model's, cannot be read, or gives a count the weights could not
/// hold (a name misread as 2511B) gives no count rather than a wrong one.
#[tokio::test]
async fn a_rejected_count_with_no_models_header_is_none() {
    let draft = hub_with_head(UNNAMED_Q8, b"draft");
    let misread = hub_with_head(UNNAMED_Q8, b"2511");
    let unread = FakeHub::default();

    for hub in [draft, misread, unread] {
        let counted =
            checked_parameters(&hub, &CountParser, REPO, Some(1.863_907_84), &unnamed()).await;
        assert_eq!(counted, None);
    }
}

/// A repository with no weights to judge by keeps the Hub's count.
#[tokio::test]
async fn a_repository_of_no_weights_keeps_the_hubs_count() {
    let projector = variants_of([("mmproj-F16.gguf", 900)]);

    let counted = checked_parameters(
        &FakeHub::default(),
        &CountParser,
        REPO,
        Some(1.0),
        &projector,
    )
    .await;

    assert_eq!(counted, Some(1.0));
}
