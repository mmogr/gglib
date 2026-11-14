//! A `HuggingFace` repository's parameter count, as the browser's summary
//! shows it.

use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::{
    largest_weights, plausible_total, sized_parameters_b, variant_head,
};
use gglib_core::ports::{GgufParserPort, HfClientPort, HfQuantInfo};

/// The parameter count, in billions, of `model_id`, whose files are listed
/// as `variants` and which the Hub counted as `hub_parameters_b`.
///
/// The count a search shows ([`sized_parameters_b`]): the Hub's while the
/// largest weights could hold it and their names agree, else the one their
/// name states (27B for `HauhauCS`, whose Hub count, 1.9B, is its draft's).
/// When neither holds, the count the head of those weights gives, as a
/// downloaded model's header is read, on the same terms; one ranged read,
/// made only then. `None` when nothing tells, and the Hub's when there are
/// no weights to judge by.
pub(crate) async fn checked_parameters(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    hub_parameters_b: Option<f64>,
    variants: &[HfQuantInfo],
) -> Option<f64> {
    let Some(weights) = largest_weights(variants) else {
        return hub_parameters_b;
    };
    if let Some(count) = sized_parameters_b(hub_parameters_b, variants) {
        return Some(count);
    }
    let head = variant_head(client, parser, model_id, weights).await?;
    head.role
        .is_none_or(GgufFileRole::is_weights)
        .then_some(head.metadata.param_count_b)
        .flatten()
        .filter(|count| plausible_total(*count, weights.total_size))
}

#[cfg(test)]
#[path = "hf_count_tests.rs"]
mod tests;
