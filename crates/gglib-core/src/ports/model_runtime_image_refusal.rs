//! The words of an image-model refusal that names more than a format string
//! can: every missing role with its command.

use crate::domain::ComponentRole;

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

/// "A", "A or B", "A, B or C".
fn either(labels: &[&str]) -> String {
    match labels {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}
