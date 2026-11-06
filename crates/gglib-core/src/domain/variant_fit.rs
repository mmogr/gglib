//! Whether a repository's variant fits this machine, said as a listing
//! says it: on the wire for the GUI's table, and in words for the CLI's.
//!
//! The verdict is the picker's ([`MemoryFit`](crate::download::MemoryFit)),
//! the one `gglib model download <repo>` chooses a variant by, so a listing
//! and the download it leads to agree. Nothing here fits anything: it
//! carries a verdict and spells it.

use serde::{Deserialize, Serialize};

use super::phone_fit::{FitVerdict, MissingInput};

/// Whether a variant fits this machine's memory budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VariantFit {
    /// It fits; `ctx` is the context it would be served at, `None` for an
    /// image model, which has none.
    Fits {
        #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
        ctx: Option<u64>,
    },
    /// Only the lowest rung of the context ladder fits.
    Tight {
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        ctx: u64,
    },
    /// It does not fit: `need_bytes` is the least memory budget whose
    /// usable share holds what it loads (its weights, and for a chat model
    /// the KV cache of the window that did not fit: the lowest rung, or
    /// the whole trained window when that is below the second rung), and
    /// `have_bytes` this machine's budget, so the two compare as like with
    /// like whichever way the verdict was reached.
    TooBig {
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        need_bytes: u64,
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        have_bytes: u64,
    },
    /// Not knowable: `missing` names what the header or the machine did
    /// not give.
    Unknown { missing: MissingInput },
}

impl VariantFit {
    /// The words a person reads beside the variant.
    #[must_use]
    pub fn word(self) -> String {
        match self {
            Self::Fits { ctx: Some(ctx) } => format!("fits with {}", context(ctx)),
            Self::Fits { ctx: None } => "fits".to_owned(),
            Self::Tight { ctx } => format!("tight at {}", context(ctx)),
            Self::TooBig { .. } => "too big".to_owned(),
            Self::Unknown { missing } => format!("unknown: {}", missing.word()),
        }
    }
}

impl From<FitVerdict> for VariantFit {
    fn from(verdict: FitVerdict) -> Self {
        match verdict {
            FitVerdict::Fits { ctx } => Self::Fits { ctx: Some(ctx) },
            FitVerdict::Tight { ctx } => Self::Tight { ctx },
            FitVerdict::TooBig {
                need_bytes,
                have_bytes,
            } => Self::TooBig {
                need_bytes,
                have_bytes,
            },
            FitVerdict::Unknown { missing } => Self::Unknown { missing },
        }
    }
}

impl MissingInput {
    /// What was missing, in words.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::TrainedContext => "trained context not read from the header",
            Self::KvShape => "KV shape not read from the header",
            Self::Weights => "no size",
            Self::Budget => "this machine's memory could not be read",
        }
    }
}

/// A context size as the listing writes it: `32K`, `4K`, or the number.
fn context(ctx: u64) -> String {
    if ctx >= 1024 && ctx.is_multiple_of(1024) {
        format!("{}K", ctx / 1024)
    } else {
        ctx.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_verdict_has_its_words() {
        assert_eq!(
            VariantFit::Fits { ctx: Some(32_768) }.word(),
            "fits with 32K"
        );
        assert_eq!(VariantFit::Fits { ctx: None }.word(), "fits");
        assert_eq!(VariantFit::Tight { ctx: 4096 }.word(), "tight at 4K");
        assert_eq!(
            VariantFit::TooBig {
                need_bytes: 2,
                have_bytes: 1
            }
            .word(),
            "too big"
        );
        assert_eq!(
            VariantFit::Unknown {
                missing: MissingInput::Budget
            }
            .word(),
            "unknown: this machine's memory could not be read"
        );
    }

    #[test]
    fn the_wire_names_the_kind_and_the_missing_input_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&VariantFit::Fits { ctx: Some(8192) }).unwrap(),
            r#"{"kind":"fits","ctx":8192}"#
        );
        assert_eq!(
            serde_json::to_string(&VariantFit::Unknown {
                missing: MissingInput::TrainedContext
            })
            .unwrap(),
            r#"{"kind":"unknown","missing":"trained_context"}"#
        );
    }

    #[test]
    fn a_launch_fit_verdict_becomes_the_same_variant_fit() {
        assert_eq!(
            VariantFit::from(FitVerdict::Tight { ctx: 4096 }),
            VariantFit::Tight { ctx: 4096 }
        );
        assert_eq!(context(1_000_000), "1000000");
    }
}
