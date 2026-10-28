//! GGUF files written in a test, a pair at a time.

/// The value types a pair here holds, as the format numbers them.
const U32: u32 = 4;
const STRING: u32 = 8;
const ARRAY: u32 = 9;

/// Where a file holds its count of pairs: after the magic, the version and
/// the tensor count.
const PAIR_COUNT_AT: usize = 16;

/// A string as a GGUF file holds it: its length, then its bytes.
fn string(text: &str) -> Vec<u8> {
    [&(text.len() as u64).to_le_bytes(), text.as_bytes()].concat()
}

/// The bytes of a GGUF v3 file of no tensors.
pub(crate) struct Gguf(Vec<u8>);

impl Gguf {
    /// A file with no pairs yet.
    pub(crate) fn new() -> Self {
        let mut bytes = b"GGUF".to_vec();
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        Self(bytes)
    }

    /// The file with one more pair, `key` and the bytes of a value of
    /// `value_type`.
    fn pair(mut self, key: &str, value_type: u32, value: &[u8]) -> Self {
        let count = &mut self.0[PAIR_COUNT_AT..][..8];
        let next = u64::from_le_bytes(count.try_into().unwrap()) + 1;
        count.copy_from_slice(&next.to_le_bytes());
        self.0.extend(string(key));
        self.0.extend_from_slice(&value_type.to_le_bytes());
        self.0.extend_from_slice(value);
        self
    }

    pub(crate) fn text(self, key: &str, value: &str) -> Self {
        self.pair(key, STRING, &string(value))
    }

    pub(crate) fn u32(self, key: &str, value: u32) -> Self {
        self.pair(key, U32, &value.to_le_bytes())
    }

    /// The file with a token list of `count` made-up tokens, as long as a
    /// real tokenizer's runs, which puts the end of the header far past a
    /// 64 KiB head.
    pub(crate) fn tokens(self, count: usize) -> Self {
        let mut value = STRING.to_le_bytes().to_vec();
        value.extend_from_slice(&(count as u64).to_le_bytes());
        for i in 0..count {
            value.extend(string(&format!("token{i}")));
        }
        self.pair("tokenizer.ggml.tokens", ARRAY, &value)
    }

    pub(crate) fn bytes(self) -> Vec<u8> {
        self.0
    }
}

/// The header of a chat model with `arch`'s fit keys: trained for
/// `trained_ctx` tokens, with 32 layers of 8 KV heads of 128 dimensions,
/// so each token costs 32 × 8 × 128 elements of K and of V. Its token list
/// follows them, as a converter writes it.
pub(crate) fn chat_model(arch: &str, trained_ctx: u32) -> Gguf {
    Gguf::new()
        .text("general.architecture", arch)
        .text("general.name", "Test chat model")
        .u32(&format!("{arch}.context_length"), trained_ctx)
        .u32(&format!("{arch}.block_count"), 32)
        .u32(&format!("{arch}.embedding_length"), 3072)
        .u32(&format!("{arch}.attention.head_count"), 24)
        .u32(&format!("{arch}.attention.head_count_kv"), 8)
        .u32(&format!("{arch}.attention.key_length"), 128)
        .u32(&format!("{arch}.attention.value_length"), 128)
        .text("tokenizer.ggml.model", "gpt2")
        .tokens(20_000)
}

/// The K and V elements a token of [`chat_model`]'s context holds.
pub(crate) const CHAT_MODEL_KV: gglib_core::domain::KvElemsPerToken =
    gglib_core::domain::KvElemsPerToken {
        k: 32 * 8 * 128,
        v: 32 * 8 * 128,
    };
