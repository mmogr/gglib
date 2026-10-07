//! `gglib model search`: the search the browser runs, and the text it prints.

use std::sync::Arc;

use clap::ValueEnum as _;

use super::super::super::test_library::run;
use super::super::test_hub::{Hub, RULE, context, text};
use super::*;
use crate::model_sort::CliHubSort;

const TIPS: [&str; 2] = [
    "💡 To download a model: gglib model download <model_id>",
    "💡 To list quantizations: gglib model download <model_id> --list-quants",
];

/// The search asks the Hub once, for the query, the limit and the order it
/// was given, and prints for the recorded page what it printed before it
/// searched through the shared function.
#[tokio::test]
async fn a_search_prints_what_it_printed_for_the_recorded_page() {
    let hub = Hub::recorded();

    let printed = found_text(&hub, "phi-4 mini", 10, HfSortField::Downloads)
        .await
        .expect("a listing");

    assert_eq!(
        printed,
        text(&[
            "",
            "📋 Found 3 models:",
            RULE,
            " 1. MaziyarPanahi/Phi-4-mini-instruct-GGUF (↓139.8K ❤16)",
            "    Quantizations: Q4_K_M, Q8_0",
            "",
            " 2. unsloth/Phi-4-mini-instruct-GGUF (↓117.1K ❤153)",
            "",
            " 3. bartowski/microsoft_Phi-4-mini-instruct-GGUF (↓75.4K ❤47)",
            "",
            TIPS[0],
            TIPS[1],
        ])
    );
    let asked = (
        Some("phi-4 mini".to_string()),
        10,
        HfSortField::Downloads,
        true,
    );
    assert_eq!(hub.asked(), [asked]);
}

/// Every hit the Hub gives is listed, whatever its ID: the Hub was asked for
/// GGUF repositories, so `owner/plain-phi` is one, though its ID names
/// neither `gguf` nor a model family. A description is cut to 80 characters,
/// and an empty one takes no line.
#[tokio::test]
async fn a_search_lists_every_hit_the_hub_gives() {
    let printed = found_text(&Hub::described(), "phi", 10, HfSortField::Downloads)
        .await
        .expect("a listing");

    assert_eq!(
        printed,
        text(&[
            "",
            "📋 Found 4 models:",
            RULE,
            " 1. owner/described-GGUF (↓1.2M ❤9)",
            "    Quantizations: Q6_K",
            &format!("    {}...", "d".repeat(77)),
            "",
            " 2. owner/accented-GGUF (↓999 ❤0)",
            "    Quantizations: Q6_K",
            &format!("    {}...", "é".repeat(77)),
            "",
            " 3. owner/plain-phi (↓1.0K ❤2)",
            "    Quantizations: Q6_K",
            "    short",
            "",
            " 4. owner/empty-GGUF (↓12 ❤3)",
            "    Quantizations: Q6_K",
            "",
            TIPS[0],
            TIPS[1],
        ])
    );
}

/// A hit's number is led by one space, however many digits it has.
#[tokio::test]
async fn a_tenth_hit_is_numbered_as_the_first_is() {
    let printed = found_text(&Hub::numbered(10), "phi", 10, HfSortField::Downloads)
        .await
        .expect("a listing");

    let numbered: Vec<&str> = printed.lines().filter(|l| l.contains("o/n")).collect();
    assert_eq!(numbered[8], " 9. o/n9 (↓9 ❤9)");
    assert_eq!(numbered[9], " 10. o/n10 (↓10 ❤10)");
}

#[tokio::test]
async fn a_search_that_finds_nothing_says_so() {
    let printed = found_text(&Hub::default(), "zzz", 10, HfSortField::Downloads).await;

    assert_eq!(printed.unwrap(), "No models found for query: 'zzz'\n");
}

/// The Hub's reason is the whole error: not an internal one of gglib's.
#[tokio::test]
async fn a_search_the_hub_refuses_fails_with_the_hubs_reason() {
    let refused = found_text(&Hub::limited(), "phi", 10, HfSortField::Downloads).await;

    assert_eq!(
        refused.unwrap_err().to_string(),
        "HF search failed: Rate limit exceeded, try again later"
    );
}

/// From the command line to the Hub: each value `--sort` accepts is asked
/// for as its own order, and the default is downloads because it is named
/// so, not because nothing else matched.
#[tokio::test]
async fn every_sort_value_is_asked_of_the_hub_as_its_own_order() {
    let accepted = [
        ("downloads", HfSortField::Downloads),
        ("likes", HfSortField::Likes),
        ("created", HfSortField::Created),
        ("updated", HfSortField::Modified),
        ("modified", HfSortField::Modified),
    ];
    // Every value clap offers is in the table above, so a new one is too.
    let offered: Vec<String> = CliHubSort::value_variants()
        .iter()
        .filter_map(clap::ValueEnum::to_possible_value)
        .map(|value| value.get_name().to_string())
        .collect();
    assert_eq!(offered, ["downloads", "likes", "created", "updated"]);

    let dir = tempfile::tempdir().unwrap();
    let hub = Arc::new(Hub::recorded());
    let ctx = context(dir.path(), &hub).await;

    for (value, _) in accepted {
        run(&ctx, &["gglib", "model", "search", "phi", "--sort", value])
            .await
            .expect("a search");
    }
    run(&ctx, &["gglib", "model", "search", "phi", "--limit", "3"])
        .await
        .expect("a search");

    let mut expected: Vec<_> = accepted
        .iter()
        .map(|(_, order)| (Some("phi".to_string()), 10, *order, true))
        .collect();
    expected.push((Some("phi".to_string()), 3, HfSortField::Downloads, true));
    assert_eq!(hub.asked(), expected);
}

/// A value `--sort` does not offer is refused by name, with the ones it
/// does, and the Hub is not asked: no value is searched as downloads but
/// `downloads`. `trending` and the Hub's own spellings used to be.
#[tokio::test]
async fn a_sort_value_search_does_not_offer_is_refused_before_the_hub_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Arc::new(Hub::recorded());
    let ctx = context(dir.path(), &hub).await;

    for value in [
        "trending",
        "id",
        "alphabetical",
        "lastModified",
        "createdAt",
        "x",
    ] {
        let refused = run(&ctx, &["gglib", "model", "search", "phi", "--sort", value])
            .await
            .expect_err("a refusal");

        let said = refused.to_string();
        assert!(
            said.contains(&format!("invalid value '{value}' for '--sort <SORT>'")),
            "{said}"
        );
        assert!(
            said.contains("[possible values: downloads, likes, created, updated]"),
            "{said}"
        );
    }
    assert_eq!(hub.asked(), []);
}

/// Every hit is a GGUF repository, so there is no flag to ask for those.
#[tokio::test]
async fn the_gguf_only_flag_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Arc::new(Hub::recorded());
    let ctx = context(dir.path(), &hub).await;

    let refused = run(&ctx, &["gglib", "model", "search", "phi", "--gguf-only"])
        .await
        .expect_err("a refusal");

    let said = refused.to_string();
    assert!(said.contains("unexpected argument '--gguf-only'"), "{said}");
    assert_eq!(hub.asked(), []);
}
