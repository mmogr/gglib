//! The names and shapes of the tensors a weights file holds.
//!
//! An image model's GGUF carries no metadata at all, so what it is can be
//! read only from the names of its tensors and their shapes. The same holds
//! for the safetensors files its components come as. This is that table, read
//! from either format, without a byte of any tensor's data.

/// One tensor a weights file declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorInfo {
    /// The tensor's name, exactly as the file spells it.
    pub name: String,
    /// The tensor's dimensions, outermost first: the order safetensors and
    /// `PyTorch` write. A GGUF file stores them innermost first (ggml's `ne`),
    /// and its reader reverses them, so one table reads the same from both.
    pub shape: Vec<u64>,
}

/// The format a tensor table was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightsFormat {
    /// A GGUF file.
    Gguf,
    /// A safetensors file.
    Safetensors,
}

/// Every tensor a weights file declares, in the order it declares them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorTable {
    /// The format the table was read from.
    pub format: WeightsFormat,
    /// A GGUF file's `general.architecture`, when it has one; always `None`
    /// for safetensors, whose header has no such field.
    pub architecture: Option<String>,
    /// The tensors, in file order.
    pub tensors: Vec<TensorInfo>,
}

impl TensorTable {
    /// The tensor named exactly `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TensorInfo> {
        self.tensors.iter().find(|tensor| tensor.name == name)
    }

    /// Whether any tensor's name starts with `prefix`.
    #[must_use]
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.tensors
            .iter()
            .any(|tensor| tensor.name.starts_with(prefix))
    }
}

/// The tensor data of a GGUF file, or of every shard of one model: its bytes,
/// and the weights they encode.
///
/// The bytes are what follows the tensor-info table, from the aligned start
/// of the data to the end of the file; the weights are the elements of every
/// tensor the table declares. Their ratio is the file's bits per weight,
/// which needs no table of what each tensor type costs and counts every
/// tensor a recipe kept at another type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TensorPayload {
    /// The bytes of tensor data.
    pub bytes: u64,
    /// The elements of every tensor.
    pub elements: u64,
}

impl TensorPayload {
    /// The bits each weight takes on average, to a thousandth, when there
    /// are weights and bytes holding them.
    #[must_use]
    pub fn bits_per_weight(self) -> Option<f64> {
        if self.bytes == 0 || self.elements == 0 {
            return None;
        }
        let millibits = u128::from(self.bytes) * 8_000 / u128::from(self.elements);
        u32::try_from(millibits)
            .ok()
            .map(|millibits| f64::from(millibits) / 1_000.0)
    }
}

impl std::ops::Add for TensorPayload {
    type Output = Self;

    /// One model's payload from two of its shards.
    fn add(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.saturating_add(other.bytes),
            elements: self.elements.saturating_add(other.elements),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_per_weight_is_the_payload_over_the_weights() {
        let payload = TensorPayload {
            bytes: 17_923_393_664,
            elements: 27_310_000_000,
        };
        let bpw = payload.bits_per_weight().unwrap();
        assert!((bpw - 5.25).abs() < 0.01, "{bpw}");
    }

    #[test]
    fn a_payload_with_no_bytes_or_no_weights_has_no_bits_per_weight() {
        assert_eq!(TensorPayload::default().bits_per_weight(), None);
        let no_bytes = TensorPayload {
            bytes: 0,
            elements: 10,
        };
        assert_eq!(no_bytes.bits_per_weight(), None);
    }

    #[test]
    fn shards_add_to_one_payload() {
        let one = TensorPayload {
            bytes: 10,
            elements: 20,
        };
        let two = TensorPayload {
            bytes: 30,
            elements: 60,
        };
        assert_eq!(
            one + two,
            TensorPayload {
                bytes: 40,
                elements: 80
            }
        );
    }

    fn table(names: &[&str]) -> TensorTable {
        TensorTable {
            format: WeightsFormat::Safetensors,
            architecture: None,
            tensors: names
                .iter()
                .map(|name| TensorInfo {
                    name: (*name).to_owned(),
                    shape: vec![1],
                })
                .collect(),
        }
    }

    #[test]
    fn get_matches_a_whole_name_only() {
        let table = table(&["model.img_in.weight"]);
        assert!(table.get("model.img_in.weight").is_some());
        assert!(table.get("img_in.weight").is_none());
        assert!(table.get("model.img_in").is_none());
    }

    #[test]
    fn has_prefix_matches_the_start_of_a_name_only() {
        let table = table(&["double_blocks.0.img_attn.qkv.weight"]);
        assert!(table.has_prefix("double_blocks."));
        assert!(!table.has_prefix("blocks."));
    }
}
