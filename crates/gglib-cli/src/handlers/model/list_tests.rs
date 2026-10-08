//! `gglib model list` as a person reads it: the table, its ID column, and
//! the line about the paired machine, on a full library and an empty one;
//! and which rows each sort and filter flag leaves, in which order.

use chrono::TimeZone as _;

use std::sync::Arc;

use super::*;
use crate::bootstrap::test_context;
use crate::handlers::model::one_shot_model_ops;
use crate::handlers::model::test_library::{Heard, Runtime, ops};

/// A catalogue row with id `id`, called `name`.
fn model(id: i64, name: &str) -> GuiModel {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "filePath": format!("/models/{name}.gguf"),
        "paramCountB": 8.0,
        "architecture": "qwen3",
        "quantization": "Q4_K_M",
        "contextLength": 32768,
        "addedAt": "2026-10-01 12:00:00",
        "hfRepoId": null,
    }))
    .unwrap()
}

#[test]
fn test_truncate_string_no_truncation_needed() {
    let result = truncate_string("short", 10);
    assert_eq!(result, "short");
}

#[test]
fn test_truncate_string_exact_length() {
    let result = truncate_string("exactly10c", 10);
    assert_eq!(result, "exactly10c");
}

#[test]
fn test_truncate_string_needs_truncation() {
    let result = truncate_string("this is a very long string", 10);
    // 9 chars of content + single-char ellipsis = 10 chars total
    assert_eq!(result, "this is a\u{2026}");
}

/// The ID column is as wide as the widest id, so a four-digit id keeps its
/// row's name in line with the rest.
#[test]
fn the_id_column_is_as_wide_as_the_widest_id() {
    let table = render_table(&[model(3, "small"), model(1000, "big")]);

    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines[0].find("Name"), lines[2].find("small"), "{table}");
    assert_eq!(lines[2].find("small"), lines[3].find("big"), "{table}");
    assert_eq!(lines[1].len(), 120 + 4, "the rule spans the wider column");
    assert!(lines[2].starts_with("3    "), "{table}");
    assert!(lines[3].starts_with("1000 "), "{table}");
}

/// Short ids get a column three wide.
#[test]
fn short_ids_get_a_column_three_wide() {
    let table = render_table(&[model(3, "small")]);

    assert!(table.starts_with("ID  Name"), "{table}");
    assert_eq!(table.lines().nth(1).map(str::len), Some(123));
}

/// The paired machine's line follows the table.
#[test]
fn the_paired_line_follows_the_table() {
    let out = listing(&[model(3, "small")], Some("desk: not connected"));

    assert!(out.starts_with("Found 1 model(s):"), "{out}");
    assert!(out.ends_with("\ndesk: not connected\n"), "{out}");
}

/// An empty library still says where the models are: a laptop with none of
/// its own, paired with a desktop that has them, is what the line is for.
#[test]
fn an_empty_library_still_names_the_paired_machine() {
    let paired = "Paired with desk (direct). Its models: gglib model list --remote";

    let out = listing(&[], Some(paired));

    assert!(out.starts_with("No models found."), "{out}");
    assert!(out.ends_with(&format!("\n{paired}\n")), "{out}");
    assert_eq!(
        listing(&[], None),
        "No models found.\nUse 'gglib model add <file_path>' to add your first model.\n"
    );
}

/// The `Images` column says `yes` for a model linked to a projector and `--`
/// for one that is not, under its header.
#[test]
fn the_images_column_marks_the_models_that_read_images() {
    let mut sees = model(1, "sees");
    sees.image_input = true;
    let table = render_table(&[sees, model(2, "blind")]);

    let lines: Vec<&str> = table.lines().collect();
    let column = lines[0].find("Images").expect("an Images header");
    assert!(lines[2][column..].starts_with("yes "), "{table}");
    assert!(lines[3][column..].starts_with("-- "), "{table}");
}

/// A library of three models, each added a day after the one before:
/// `alpha` (7.5B, tagged `chat`), `bravo` (3B) and `charlie` (27B).
async fn three_models(dir: &std::path::Path) -> CliContext {
    gglib_core::paths::isolate_data_root();
    let ctx = test_context(dir).await;
    for (day, name, params, context) in [
        (1, "alpha", 7.5, 32_768),
        (2, "bravo", 3.0, 8_192),
        (3, "charlie", 27.0, 131_072),
    ] {
        let added = chrono::Utc
            .with_ymd_and_hms(2026, 10, day, 12, 0, 0)
            .unwrap();
        let path = format!("/models/{name}.gguf").into();
        let mut model = gglib_core::NewModel::new(name.to_owned(), path, params, added);
        model.context_length = Some(context);
        ctx.app.models().add(model).await.unwrap();
    }
    ctx.app
        .models()
        .add_tag(1, "chat".to_owned())
        .await
        .unwrap();
    ctx
}

/// What `gglib model list <flags>` prints through `ops`, with no machine
/// paired.
async fn printed(ops: &ModelOps, flags: &[&str]) -> String {
    use clap::Parser as _;
    let line = [&["gglib", "model", "list"], flags].concat();
    let cli = crate::Cli::try_parse_from(line).expect("the flags parse");
    let Some(crate::Commands::Model {
        command: crate::ModelCommand::List(args),
    }) = cli.command
    else {
        panic!("{flags:?} is not a list");
    };
    listing(&fetch_models(ops, &args).await.unwrap(), None)
}

/// The listing with no flags, whole: newest first, every column.
#[tokio::test]
async fn the_library_is_listed_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = three_models(dir.path()).await;

    let rule = "-".repeat(123);
    assert_eq!(
        printed(&one_shot_model_ops(&ctx), &[]).await,
        format!(
            "Found 3 model(s):

ID  Name                      Params   Arch         Quant    Context    Images  Added                File Path
{rule}
3   charlie                   27.0     --           --       131072     --      2026-10-03 12:00:00  /models/charlie.gguf
2   bravo                     3.0      --           --       8192       --      2026-10-02 12:00:00  /models/bravo.gguf
1   alpha                     7.5      --           --       32768      --      2026-10-01 12:00:00  /models/alpha.gguf
"
        )
    );
}

/// Each sort and filter flag, and the rows it leaves in the order it leaves
/// them. A filter that leaves none prints the empty library's two lines.
#[tokio::test]
async fn each_sort_and_filter_flag_leaves_the_rows_it_did_before() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = three_models(dir.path()).await;

    let table: [(&[&str], &[&str]); 12] = [
        (
            &["--sort", "added", "--order", "asc"],
            &["alpha", "bravo", "charlie"],
        ),
        (
            &["--sort", "name", "--order", "asc"],
            &["alpha", "bravo", "charlie"],
        ),
        (&["--sort", "name"], &["charlie", "bravo", "alpha"]),
        (
            &["--sort", "params", "--order", "desc"],
            &["charlie", "alpha", "bravo"],
        ),
        (
            &["--sort", "params", "--order", "asc"],
            &["bravo", "alpha", "charlie"],
        ),
        (&["--min-params", "5"], &["charlie", "alpha"]),
        (&["--max-params", "5"], &["bravo"]),
        (
            &["--min-params", "2", "--max-params", "9", "--sort", "name"],
            &["bravo", "alpha"],
        ),
        (&["--tag", "chat"], &["alpha"]),
        (&["--tag", "chat", "--tag", "code"], &[]),
        (&["--tag", "chat", "--max-params", "5"], &[]),
        (&["--min-speed", "1"], &[]),
    ];
    let ops = one_shot_model_ops(&ctx);
    for (flags, names) in table {
        let text = printed(&ops, flags).await;
        if names.is_empty() {
            assert_eq!(text, listing(&[], None), "{flags:?}");
            continue;
        }
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            format!("Found {} model(s):", names.len()),
            "{flags:?}"
        );
        let rows = lines[4..].iter().map(|row| row.split_whitespace().nth(1));
        let rows: Vec<&str> = rows.map(Option::unwrap).collect();
        assert_eq!(rows, names, "{flags:?}\n{text}");
    }
}

/// The rows are `ModelOps`' own: a model its runtime is serving is listed as
/// being served, on the port it is served from.
#[tokio::test]
async fn the_rows_are_the_ones_modelops_lists() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = three_models(dir.path()).await;
    let serving = Arc::new(Runtime::serving(2, 9001));
    let ops = ops(&ctx, &Arc::new(Heard::default()), &serving);
    let args = ListArgs {
        sort: crate::model_sort::CliModelSortBy::Name,
        order: crate::model_sort::CliSortOrder::Asc,
        min_params: None,
        max_params: None,
        min_speed: None,
        max_speed: None,
        tags: Vec::new(),
    };

    let models = fetch_models(&ops, &args).await.unwrap();

    let served: Vec<_> = models.iter().map(|m| (m.is_serving, m.port)).collect();
    assert_eq!(served, [(false, None), (true, Some(9001)), (false, None)]);
}
