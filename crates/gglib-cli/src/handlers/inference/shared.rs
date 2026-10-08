//! What `serve` says on stderr of the launch it was asked for.

use crate::presentation::sampling_values::stated_parameters;
use gglib_core::domain::InferenceConfig;

/// Log mlock status to stderr.
pub(crate) fn log_mlock_info(mlock: bool) {
    if mlock {
        eprintln!("  Memory lock: enabled");
    }
}

/// Log the sampling parameters the operator stated, to stderr.
///
/// From [`stated_parameters`], which reads the patch gglib puts on the wire
/// rather than naming fields: a banner that under-reports what it applies is
/// the same class of bug as one that over-reports it.
pub(crate) fn log_inference_info(config: &InferenceConfig) {
    let stated = stated_parameters(config);
    if stated.is_empty() {
        return;
    }

    eprintln!("  Inference parameters:");
    for (field, value) in stated {
        eprintln!("    {}: {value}", field.replace('_', "-"));
    }
}
