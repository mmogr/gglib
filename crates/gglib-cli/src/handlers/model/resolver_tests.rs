//! A miss in this catalogue, with and without a pairing.

use gglib_core::{RemotePairing, Settings};

use super::*;
use crate::bootstrap::test_context;

/// How the context's machine stands: unpaired, or paired with a machine
/// that gave a name or none.
enum Paired<'a> {
    No,
    With(Option<&'a str>),
}

/// The CLI's context over an empty catalogue in `dir`, `paired` as asked.
async fn context(dir: &tempfile::TempDir, paired: Paired<'_>) -> CliContext {
    let ctx = test_context(dir.path()).await;
    if let Paired::With(name) = paired {
        let pairing = RemotePairing {
            ticket: "pipe-desk".to_owned(),
            api_key: "key".to_owned(),
            default_model: None,
            port: None,
            name: name.map(str::to_owned),
        };
        ctx.settings_repo
            .modify(&|settings: &mut Settings| {
                settings.remote_pairing = Some(pairing.clone());
                Ok(())
            })
            .await
            .expect("paired");
    }
    ctx
}

/// `gglib chat 3` with no model 3 here, paired with desk: the miss says
/// where desk's models are, in the command that lists them.
#[tokio::test]
async fn a_miss_while_paired_points_at_the_paired_machine() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Paired::With(Some("desk"))).await;

    let text = resolve_for(&ctx, "3", ModelAction::Chat)
        .await
        .expect_err("no model 3 here")
        .to_string();

    assert_eq!(
        text,
        "no model 3 here; desk's models need --remote (gglib model list --remote)"
    );
}

/// A paired machine that has given no name is called what every surface
/// calls it.
#[tokio::test]
async fn an_unnamed_paired_machine_is_named_as_everywhere_else() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Paired::With(None)).await;

    let text = resolve_for(&ctx, "qwen3", ModelAction::Load)
        .await
        .expect_err("no qwen3 here")
        .to_string();

    assert!(
        text.contains("the paired machine's models need --remote"),
        "{text}"
    );
}

/// A command that changes this machine's library is not pointed at
/// `--remote`, which it refuses; nor is any command on an unpaired machine.
#[tokio::test]
async fn the_hint_is_only_for_what_remote_reaches_and_only_when_paired() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paired = context(&dir, Paired::With(Some("desk"))).await;
    let manage = resolve_model_identifier(&paired, "3")
        .await
        .expect_err("no model 3 here")
        .to_string();
    assert!(
        manage.starts_with("No model found matching: '3'"),
        "{manage}"
    );

    let lone = tempfile::tempdir().expect("tempdir");
    let unpaired = context(&lone, Paired::No).await;
    let chat = resolve_for(&unpaired, "3", ModelAction::Chat)
        .await
        .expect_err("no model 3 here")
        .to_string();
    assert!(chat.starts_with("No model found matching: '3'"), "{chat}");
    assert!(!chat.contains("--remote"), "{chat}");
}
