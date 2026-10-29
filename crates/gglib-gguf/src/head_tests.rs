//! Tests for [`super`]: a head keeps the whole pairs before its cut wherever
//! it is cut, and a head the size a download asks for holds the keys a fit
//! is judged by.

use std::path::Path;

use gglib_core::{GgufParseError, GgufParserPort, HeadMetadata};

use crate::GgufParser;
use crate::parser::parse_internal;
use crate::reader::GgufReader;

const U32: u32 = 4;
const I32: u32 = 5;
const F32: u32 = 6;
const STRING: u32 = 8;
const ARRAY: u32 = 9;

/// The head a download reads of a file before it fetches the file.
const HEAD_BYTES: usize = 64 * 1024;

/// Where a file holds its metadata count: after the magic, the version and
/// the tensor count.
const METADATA_COUNT_AT: usize = 16;

/// A string as a file holds it: its length, then its bytes.
fn string(text: &str) -> Vec<u8> {
    [&(text.len() as u64).to_le_bytes(), text.as_bytes()].concat()
}

/// The bytes of a GGUF v3 file, written a pair at a time.
struct Gguf(Vec<u8>);

impl Gguf {
    /// A file that declares `tensor_count` tensors, and no pairs yet.
    fn new(tensor_count: u64) -> Self {
        let mut bytes = b"GGUF".to_vec();
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&tensor_count.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        Self(bytes)
    }

    /// The file with one more pair: `key`, and `value` as the bytes of a
    /// value of `value_type`.
    fn pair(mut self, key: &str, value_type: u32, value: &[u8]) -> Self {
        let count = &mut self.0[METADATA_COUNT_AT..][..8];
        let next = u64::from_le_bytes(count.try_into().unwrap()) + 1;
        count.copy_from_slice(&next.to_le_bytes());
        self.0.extend(string(key));
        self.0.extend_from_slice(&value_type.to_le_bytes());
        self.0.extend_from_slice(value);
        self
    }

    fn text(self, key: &str, value: &str) -> Self {
        self.pair(key, STRING, &string(value))
    }

    fn u32(self, key: &str, value: u32) -> Self {
        self.pair(key, U32, &value.to_le_bytes())
    }

    fn f32(self, key: &str, value: f32) -> Self {
        self.pair(key, F32, &value.to_le_bytes())
    }

    /// The file with an array of `elements`, each the bytes of a value of
    /// `element_type`.
    fn array(self, key: &str, element_type: u32, elements: &[Vec<u8>]) -> Self {
        let mut value = element_type.to_le_bytes().to_vec();
        value.extend_from_slice(&(elements.len() as u64).to_le_bytes());
        value.extend(elements.concat());
        self.pair(key, ARRAY, &value)
    }
}

/// A small file with a value of each shape a model's metadata has, the
/// tokenizer's arrays after the architecture's keys as converters write
/// them.
fn small() -> Vec<u8> {
    let tokens = ["<s>", "</s>", "hello", "多语言"].map(string);
    let types = [1_i32, 3, 1, 1].map(|t| t.to_le_bytes().to_vec());
    Gguf::new(2)
        .text("general.architecture", "llama")
        .text("general.name", "Tiny 多语言")
        .u32("llama.block_count", 5)
        .u32("llama.context_length", 2048)
        .f32("llama.rope.freq_base", 10_000.0)
        .text("tokenizer.ggml.model", "llama")
        .array("tokenizer.ggml.tokens", STRING, &tokens)
        .array("tokenizer.ggml.token_type", I32, &types)
        .u32("tokenizer.ggml.eos_token_id", 2)
        .0
}

/// The header of unsloth's `Qwen3-0.6B-Q4_K_M.gguf`: its pairs, in its
/// order and with its values, up to and through its token list, whose
/// 151,936 tokens stand in for the real ones at a similar length.
fn qwen3_header() -> Vec<u8> {
    let tokens: Vec<Vec<u8>> = (0..151_936).map(|i| string(&format!("tok{i}"))).collect();
    Gguf::new(310)
        .text("general.architecture", "qwen3")
        .text("general.type", "model")
        .text("general.name", "Qwen3-0.6B")
        .text("general.basename", "Qwen3-0.6B")
        .text("general.quantized_by", "Unsloth")
        .text("general.size_label", "0.6B")
        .text("general.repo_url", "https://huggingface.co/unsloth")
        .u32("qwen3.block_count", 28)
        .u32("qwen3.context_length", 40_960)
        .u32("qwen3.embedding_length", 1024)
        .u32("qwen3.feed_forward_length", 3072)
        .u32("qwen3.attention.head_count", 16)
        .u32("qwen3.attention.head_count_kv", 8)
        .f32("qwen3.rope.freq_base", 1_000_000.0)
        .f32("qwen3.attention.layer_norm_rms_epsilon", 1e-6)
        .u32("qwen3.attention.key_length", 128)
        .u32("qwen3.attention.value_length", 128)
        .text("tokenizer.ggml.model", "gpt2")
        .text("tokenizer.ggml.pre", "qwen2")
        .array("tokenizer.ggml.tokens", STRING, &tokens)
        .text("tokenizer.chat_template", "{{ messages }}")
        .0
}

fn head_of(bytes: &[u8], file_name: &str) -> Result<HeadMetadata, GgufParseError> {
    GgufParser::new().parse_head(bytes, file_name)
}

/// What a whole parse reads from `bytes`.
fn whole(bytes: &[u8], file_name: &str) -> gglib_core::GgufMetadata {
    parse_internal(GgufReader::over(bytes), Path::new(file_name)).unwrap()
}

/// Every prefix of a file from its magic on is read, never refused, and
/// holds a prefix of its pairs: as many as end inside it, each as the whole
/// file has it. The bytes it says it used read back to the same pairs.
#[test]
fn a_head_cut_anywhere_keeps_the_whole_pairs_before_the_cut() {
    let bytes = small();
    let full = whole(&bytes, "tiny.gguf").metadata;
    let mut pairs_before = 0;

    for cut in 0..=bytes.len() {
        let read = head_of(&bytes[..cut], "tiny.gguf");
        if cut < 4 {
            assert!(read.is_err(), "{cut} bytes, no magic, were read: {read:?}");
            continue;
        }
        let read = read.unwrap_or_else(|error| panic!("a head of {cut} bytes: {error}"));
        let pairs = &read.metadata.metadata;

        assert!(pairs.len() >= pairs_before, "a longer head lost a pair");
        assert!(
            pairs
                .iter()
                .all(|(key, value)| full.get(key) == Some(value)),
            "a head of {cut} bytes read a pair the file does not hold: {pairs:?}"
        );
        assert_eq!(read.complete, cut == bytes.len(), "at {cut}");
        assert!(read.bytes_used <= cut as u64);
        let used = usize::try_from(read.bytes_used).unwrap();
        if used >= 4 {
            assert_eq!(
                &head_of(&bytes[..used], "tiny.gguf")
                    .unwrap()
                    .metadata
                    .metadata,
                pairs
            );
        }
        pairs_before = pairs.len();
    }
    assert_eq!(pairs_before, full.len());
}

/// The head of a file the size a download reads holds every key a fit is
/// judged by: the tokenizer's arrays come after them, and a token list that
/// runs past the head ends the read without failing it.
#[test]
fn the_fit_keys_survive_a_64_kib_head_of_a_real_models_header() {
    let bytes = qwen3_header();
    assert!(bytes.len() > HEAD_BYTES * 16, "{} bytes", bytes.len());

    let read = head_of(&bytes[..HEAD_BYTES], "Qwen3-0.6B-Q4_K_M.gguf").unwrap();

    assert!(read.reached_tokenizer);
    assert!(!read.complete);
    // Where the token list starts in unsloth's file, read off its first
    // 64 KiB.
    assert_eq!(read.bytes_used, 847);
    let metadata = &read.metadata;
    assert_eq!(metadata.architecture.as_deref(), Some("qwen3"));
    assert_eq!(metadata.context_length, Some(40_960));
    assert_eq!(metadata.quantization.as_deref(), Some("Q4_K_M"));
    assert!(
        metadata
            .param_count_b
            .is_some_and(|b| (b - 0.6).abs() < 1e-9)
    );
    for (key, value) in [
        ("qwen3.block_count", "28"),
        ("qwen3.attention.head_count", "16"),
        ("qwen3.attention.head_count_kv", "8"),
        ("qwen3.attention.key_length", "128"),
        ("qwen3.attention.value_length", "128"),
        ("qwen3.embedding_length", "1024"),
    ] {
        assert_eq!(
            metadata.metadata.get(key).map(String::as_str),
            Some(value),
            "{key}"
        );
    }
    assert!(!metadata.metadata.contains_key("tokenizer.chat_template"));
}

/// A head and the whole file agree on every key of the architecture, and on
/// what is extracted from them.
#[test]
fn a_head_and_the_whole_file_agree_on_the_architectures_keys() {
    let bytes = qwen3_header();
    let name = "Qwen3-0.6B-Q4_K_M.gguf";
    let full = whole(&bytes, name);
    let head = head_of(&bytes[..HEAD_BYTES], name).unwrap().metadata;

    let of_arch = |metadata: &gglib_core::GgufMetadata| {
        let mut keys: Vec<(String, String)> = metadata
            .metadata
            .iter()
            .filter(|(key, _)| key.starts_with("qwen3."))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        keys.sort();
        keys
    };
    assert_eq!(of_arch(&head).len(), 10);
    assert_eq!(of_arch(&head), of_arch(&full));
    assert_eq!(
        (head.architecture, head.context_length, head.role),
        (full.architecture, full.context_length, full.role)
    );
}

/// A file may write its tokenizer first, as llama.cpp's small test models
/// do; reaching the tokenizer then says nothing of the keys after it, and
/// only `complete` says whether a missing key is missing from the file.
#[test]
fn reaching_the_tokenizer_says_nothing_of_the_keys_after_it() {
    let tokens = ["<unk>", "<s>", "</s>"].map(string);
    let bytes = Gguf::new(0)
        .array("tokenizer.ggml.tokens", STRING, &tokens)
        .text("general.architecture", "llama")
        .u32("llama.context_length", 2048)
        .0;

    let cut = head_of(&bytes[..bytes.len() - 1], "stories.gguf").unwrap();
    let whole = head_of(&bytes, "stories.gguf").unwrap();

    assert!(cut.reached_tokenizer && !cut.complete);
    assert_eq!(cut.metadata.context_length, None);
    assert!(whole.reached_tokenizer && whole.complete);
    assert_eq!(whole.metadata.context_length, Some(2048));
}

/// A file the head holds whole is read whole, and so is one with no
/// tokenizer at all, such as a later shard of a split model.
#[test]
fn a_head_that_holds_the_whole_metadata_is_complete() {
    let bytes = Gguf::new(0)
        .text("split.no", "1")
        .text("split.count", "2")
        .0;

    let read = head_of(&bytes, "model-00002-of-00002.gguf").unwrap();

    assert!(read.complete && !read.reached_tokenizer);
    assert_eq!(read.bytes_used, bytes.len() as u64);
    assert_eq!(read.metadata.metadata.len(), 2);
}

/// A head that ends inside the header holds no pair and has used none of
/// its bytes, though it is a GGUF file.
#[test]
fn a_head_that_ends_inside_the_header_holds_nothing() {
    let bytes = small();

    let read = head_of(&bytes[..10], "tiny.gguf").unwrap();

    assert!(read.metadata.metadata.is_empty());
    assert!(!read.complete && !read.reached_tokenizer);
    assert_eq!(read.bytes_used, 0);
}

/// What is not a GGUF file is refused, cut or not, and so is a malformed
/// value inside the head: only running out is taken for a cut.
#[test]
fn a_head_that_is_no_gguf_or_is_malformed_is_refused() {
    let refusal = |bytes: &[u8]| match head_of(bytes, "x.gguf") {
        Err(GgufParseError::InvalidFormat(message)) => message,
        other => panic!("not refused as malformed: {other:?}"),
    };
    let mut wrong_version = small();
    wrong_version[4] = 9;
    let unknown_type = Gguf::new(0).pair("general.name", 13, &[0; 8]).0;
    let not_utf8 = Gguf::new(0)
        .pair("general.name", STRING, &[1, 0, 0, 0, 0, 0, 0, 0, 0xFF])
        .0;

    assert_eq!(refusal(b"GGML and more"), "Invalid magic number");
    assert_eq!(refusal(&wrong_version), "Unsupported version: 9");
    assert_eq!(refusal(&unknown_type), "Unknown value type: 13");
    assert_eq!(refusal(&not_utf8), "Invalid UTF-8 string");
    assert!(matches!(
        head_of(b"GGU", "x.gguf"),
        Err(GgufParseError::Io(_))
    ));
}
