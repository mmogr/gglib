//! `gglib model list --remote`: the paired machine's catalogue, as its proxy
//! publishes it.
//!
//! A `#[path]` child of `list.rs`, which sits near its size budget. What is
//! listed is `GET /v1/models` through the tunnel — the same answer any
//! client of that machine gets — so the columns are the ones that answer
//! carries: the name a turn is asked for by, and the context it would be
//! served with. The sort and filter flags describe this machine's
//! catalogue and are not applied; the far list is short and arrives sorted
//! as that machine sorts it.

use anyhow::Result;
use serde::Deserialize;

use crate::bootstrap::CliContext;
use crate::target::Target;

/// What `/v1/models` answers, as far as this listing reads it.
#[derive(Debug, Deserialize)]
struct Published {
    data: Vec<FarModel>,
}

/// One model as the far proxy advertises it.
#[derive(Debug, Deserialize)]
pub(super) struct FarModel {
    /// The name a turn asks for it by — `{model}:{profile}` for a variant.
    id: String,
    /// The context it would be served with, when the proxy says.
    #[serde(default)]
    context_window: Option<u64>,
}

pub(super) async fn execute(ctx: &CliContext, target: Target) -> Result<()> {
    let far = target.far(ctx).await?;
    let published: Published = far.get_json("/models").await?;
    if published.data.is_empty() {
        println!("The machine {} serves no models.", far.fingerprint);
        return Ok(());
    }
    println!(
        "{} model(s) on {} ({}):\n",
        published.data.len(),
        far.fingerprint,
        far.path
    );
    print!("{}", render(&published.data));
    println!("\nName one to a turn:  gglib chat --remote <name>");
    Ok(())
}

/// The table, as text, so it can be checked without a machine.
fn render(models: &[FarModel]) -> String {
    let width = models.iter().map(|m| m.id.len()).max().unwrap_or(4).max(4);
    let mut out = format!("{:<width$}  {:>9}\n", "NAME", "CONTEXT");
    out.push_str(&"-".repeat(width + 11));
    out.push('\n');
    for model in models {
        let context = model
            .context_window
            .map_or_else(|| "-".to_owned(), |c| c.to_string());
        out.push_str(&format!("{:<width$}  {context:>9}\n", model.id));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_far_list_shows_the_name_a_turn_uses_and_the_context_it_gets() {
        let models = vec![
            FarModel {
                id: "qwen3".to_owned(),
                context_window: Some(30_000),
            },
            FarModel {
                id: "qwen3:coding".to_owned(),
                context_window: None,
            },
        ];
        let table = render(&models);
        assert!(table.starts_with("NAME"), "{table}");
        assert!(table.contains("qwen3         "), "{table}");
        assert!(table.contains("30000"), "{table}");
        assert!(table.contains("qwen3:coding          -"), "{table}");
    }

    /// The far side speaks the OpenAI shape and may say more than this
    /// listing reads; what it says extra must not break the listing.
    #[test]
    fn a_published_list_with_fields_this_build_does_not_know_still_reads() {
        let json = r#"{"object":"list","data":[{"id":"m","object":"model","created":1,"owned_by":"gglib","context_window":4096,"capabilities":["embeddings"]}]}"#;
        let published: Published = serde_json::from_str(json).unwrap();
        assert_eq!(published.data[0].id, "m");
        assert_eq!(published.data[0].context_window, Some(4096));
    }
}
