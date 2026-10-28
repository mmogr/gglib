//! The secret `gglib config settings reveal` takes, by the name core gives it.

use clap::ValueEnum;
use clap::builder::PossibleValue;
use gglib_core::secret::SettingSecret;

/// A [`SettingSecret`], named on the command line by
/// [`SettingSecret::name`]. Written by hand because clap's derive takes a
/// literal name per variant, and core's `name` is the one spelling of each:
/// `--help`, completions and the refusal of an unknown key all read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretArg(pub SettingSecret);

/// [`SettingSecret::ALL`], each wrapped.
const VARIANTS: [SecretArg; SettingSecret::ALL.len()] = {
    let mut variants = [SecretArg(SettingSecret::ALL[0]); SettingSecret::ALL.len()];
    let mut i = 1;
    while i < variants.len() {
        variants[i] = SecretArg(SettingSecret::ALL[i]);
        i += 1;
    }
    variants
};

impl ValueEnum for SecretArg {
    fn value_variants<'a>() -> &'a [Self] {
        &VARIANTS
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.0.name()))
    }
}
