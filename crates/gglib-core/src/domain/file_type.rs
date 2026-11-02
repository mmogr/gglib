//! The type a GGUF file was quantized to, as its header states it.
//!
//! llama.cpp's converter and quantizer write `general.file_type`, a
//! `llama_ftype` (`include/llama.h`). It names the recipe the file was made
//! with, and that is all it names: an uploader's `UD-Q4_K_XL`, `Q4_K_L` and
//! `Q4_K_P` files all state `Q4_K_M`, and the tensors a recipe kept at
//! another type show only in the file's bits per weight
//! ([`TensorPayload`](super::tensor_table::TensorPayload)).
//!
//! The numbering is llama.cpp's own and is not ggml's tensor-type numbering:
//! file type 7 is `Q8_0`, where tensor type 7 is `Q5_1`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A `general.file_type` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileType(pub u32);

impl FileType {
    /// The name llama.cpp's quantizer gives this type, as an uploader would
    /// write it (`Q4_K_M`), when the pinned llama.cpp knows it.
    ///
    /// The table is `llama_ftype` at the pinned release, without the
    /// numbers llama.cpp has retired (4–6, 33–35).
    #[must_use]
    pub const fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0 => "F32",
            1 => "F16",
            2 => "Q4_0",
            3 => "Q4_1",
            7 => "Q8_0",
            8 => "Q5_0",
            9 => "Q5_1",
            10 => "Q2_K",
            11 => "Q3_K_S",
            12 => "Q3_K_M",
            13 => "Q3_K_L",
            14 => "Q4_K_S",
            15 => "Q4_K_M",
            16 => "Q5_K_S",
            17 => "Q5_K_M",
            18 => "Q6_K",
            19 => "IQ2_XXS",
            20 => "IQ2_XS",
            21 => "Q2_K_S",
            22 => "IQ3_XS",
            23 => "IQ3_XXS",
            24 => "IQ1_S",
            25 => "IQ4_NL",
            26 => "IQ3_S",
            27 => "IQ3_M",
            28 => "IQ2_S",
            29 => "IQ2_M",
            30 => "IQ4_XS",
            31 => "IQ1_M",
            32 => "BF16",
            36 => "TQ1_0",
            37 => "TQ2_0",
            38 => "MXFP4_MOE",
            39 => "NVFP4",
            40 => "Q1_0",
            41 => "Q2_0",
            _ => return None,
        })
    }

    /// Whether the file's weights were quantized: every type but the float
    /// ones a converter writes before any quantizing, `F32`, `F16` and
    /// `BF16`. A number the pinned llama.cpp does not know counts as
    /// quantized, since every type llama.cpp has added since `BF16` is one.
    #[must_use]
    pub const fn is_quantized(self) -> bool {
        !matches!(self.0, 0 | 1 | 32)
    }
}

impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => write!(f, "file type {}", self.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers ggml's tensor types use for other formats: a table built
    /// from those reads a `Q8_0` file as `Q5_1`.
    #[test]
    fn the_numbers_are_llama_file_types_not_tensor_types() {
        assert_eq!(FileType(7).name(), Some("Q8_0"));
        assert_eq!(FileType(8).name(), Some("Q5_0"));
        assert_eq!(FileType(9).name(), Some("Q5_1"));
    }

    #[test]
    fn the_k_quants_and_the_newer_types_are_named() {
        assert_eq!(FileType(15).name(), Some("Q4_K_M"));
        assert_eq!(FileType(18).name(), Some("Q6_K"));
        assert_eq!(FileType(30).name(), Some("IQ4_XS"));
        assert_eq!(FileType(32).name(), Some("BF16"));
        assert_eq!(FileType(38).name(), Some("MXFP4_MOE"));
        assert_eq!(FileType(41).name(), Some("Q2_0"));
    }

    #[test]
    fn only_the_float_types_are_unquantized() {
        for float in [0, 1, 32] {
            assert!(!FileType(float).is_quantized(), "{float}");
        }
        for quantized in [2, 7, 15, 18, 30, 38, 41, 42] {
            assert!(FileType(quantized).is_quantized(), "{quantized}");
        }
    }

    #[test]
    fn a_retired_or_unknown_number_has_no_name_and_says_its_number() {
        for retired in [4, 5, 6, 33, 34, 35, 42] {
            assert_eq!(FileType(retired).name(), None, "{retired}");
        }
        assert_eq!(FileType(42).to_string(), "file type 42");
        assert_eq!(FileType(15).to_string(), "Q4_K_M");
    }
}
