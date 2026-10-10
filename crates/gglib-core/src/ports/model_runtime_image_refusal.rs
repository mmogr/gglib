//! The words of two image-model refusals, which name more than a format
//! string can: every missing role with its command, and the bytes.

use crate::domain::ComponentRole;
use crate::download::format_size;

/// "Image model 'flux' has no VAE or T5-XXL linked …", with the command that
/// links each.
pub(super) fn incomplete(model: &str, missing: &[ComponentRole]) -> String {
    let labels: Vec<&str> = missing.iter().map(|role| role.label()).collect();
    let flags: Vec<String> = missing
        .iter()
        .map(|role| format!("--component {}=<path>", role.as_str()))
        .collect();
    format!(
        "Image model '{model}' has no {} linked, so it cannot draw. Link {} with \
         `gglib model update \"{model}\" {}`.",
        either(&labels),
        if missing.len() == 1 { "it" } else { "each" },
        flags.join(" ")
    )
}

/// "Image model 'flux' needs 27.92 GiB; 9.13 GiB is free beside 'qwen', which
/// a chat is using …", saying so where either size is unknown.
pub(super) fn does_not_fit(
    model: &str,
    held_model: &str,
    needed_bytes: Option<u64>,
    free_bytes: Option<u64>,
) -> String {
    let needed = needed_bytes.map_or_else(
        || "needs more memory than is free".to_owned(),
        |needed| format!("needs {}", format_size(needed)),
    );
    let free = free_bytes.map_or_else(
        || format!("how much is free beside '{held_model}' cannot be read"),
        |free| format!("{} is free beside '{held_model}'", format_size(free)),
    );
    format!(
        "Image model '{model}' {needed}; {free}, which a chat is using. Use a smaller family, \
         stop the chat model, or draw from a chat on the paired machine."
    )
}

/// "A", "A or B", "A, B or C".
fn either(labels: &[&str]) -> String {
    match labels {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}
