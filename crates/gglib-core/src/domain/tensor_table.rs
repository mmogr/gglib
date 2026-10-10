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

#[cfg(test)]
mod tests {
    use super::*;

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
