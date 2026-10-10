//! `gglib model browse`: a category's search, and the text it prints.

use std::sync::Arc;

use clap::ValueEnum as _;
use gglib_core::ports::{HfModelKind, HfSortField};

use super::super::super::test_library::run;
use super::super::test_hub::{Hub, RULE, context, text};
use super::*;

const TIPS: [&str; 2] = [
    "💡 To download a model: gglib model download <model_id>",
    "💡 To see all quantizations: gglib model download <model_id> --list-quants",
];

/// A browse asks the Hub once, for `gguf` in the category's order, and
/// prints for the recorded page what it printed before it searched through
/// the shared function.
#[tokio::test]
async fn a_browse_prints_what_it_printed_for_the_recorded_page() {
    let hub = Hub::recorded();

    let printed = found_text(
        &hub,
        CliBrowseCategory::Popular,
        20,
        None,
        HfModelKind::Chat,
    )
    .await
    .expect("a listing");

    assert_eq!(
        printed,
        text(&[
            "",
            "🏆 POPULAR GGUF Models:",
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
    let asked = (Some("gguf".to_string()), 20, HfSortField::Downloads, true);
    assert_eq!(hub.asked(), [asked]);
}

/// The recent models of a size are the newest of a search for `gguf` and
/// that size. A description is cut to 100 characters.
#[tokio::test]
async fn a_browse_of_a_size_searches_for_it_and_cuts_descriptions_to_100() {
    let hub = Hub::described();

    let printed = found_text(
        &hub,
        CliBrowseCategory::Recent,
        5,
        Some("7B".to_string()),
        HfModelKind::Chat,
    )
    .await
    .expect("a listing");

    assert_eq!(
        printed,
        text(&[
            "",
            "🏆 RECENT GGUF Models:",
            RULE,
            " 1. owner/described-GGUF (↓1.2M ❤9)",
            "    Quantizations: Q6_K",
            &format!("    {}...", "d".repeat(97)),
            "",
            " 2. owner/accented-GGUF (↓999 ❤0)",
            "    Quantizations: Q6_K",
            &format!("    {}...", "é".repeat(97)),
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
    let asked = (Some("gguf 7B".to_string()), 5, HfSortField::Created, true);
    assert_eq!(hub.asked(), [asked]);
}

/// A hit's number fills two columns.
#[tokio::test]
async fn a_tenth_hit_is_numbered_in_the_columns_of_the_first() {
    let printed = found_text(
        &Hub::numbered(10),
        CliBrowseCategory::Popular,
        20,
        None,
        HfModelKind::Chat,
    )
    .await
    .expect("a listing");

    let numbered: Vec<&str> = printed.lines().filter(|l| l.contains("o/n")).collect();
    assert_eq!(numbered[8], " 9. o/n9 (↓9 ❤9)");
    assert_eq!(numbered[9], "10. o/n10 (↓10 ❤10)");
}

#[tokio::test]
async fn a_browse_that_finds_nothing_says_so() {
    let printed = found_text(
        &Hub::default(),
        CliBrowseCategory::Recent,
        20,
        None,
        HfModelKind::Chat,
    )
    .await;

    assert_eq!(printed.unwrap(), "No recent models found.\n");
}

/// From the command line to the Hub: each category is its own order, and
/// the one browsed when none is named is the popular one.
#[tokio::test]
async fn every_category_is_asked_of_the_hub_as_its_own_order() {
    let offered: Vec<String> = CliBrowseCategory::value_variants()
        .iter()
        .filter_map(clap::ValueEnum::to_possible_value)
        .map(|value| value.get_name().to_string())
        .collect();
    assert_eq!(offered, ["popular", "recent"]);
    let printed_as: Vec<&str> = CliBrowseCategory::value_variants()
        .iter()
        .map(|category| category.name())
        .collect();
    assert_eq!(printed_as, offered, "a category is printed as it is typed");

    let dir = tempfile::tempdir().unwrap();
    let hub = Arc::new(Hub::recorded());
    let ctx = context(dir.path(), &hub).await;

    for argv in [
        &["gglib", "model", "browse", "popular"][..],
        &["gglib", "model", "browse", "recent"],
        &["gglib", "model", "browse"],
    ] {
        run(&ctx, argv).await.expect("a browse");
    }

    let gguf = || Some("gguf".to_string());
    assert_eq!(
        hub.asked(),
        [
            (gguf(), 20, HfSortField::Downloads, true),
            (gguf(), 20, HfSortField::Created, true),
            (gguf(), 20, HfSortField::Downloads, true),
        ]
    );
}

/// A category `browse` does not offer is refused by name, with the ones it
/// does, and the Hub is not asked. `trending`, and any other word, used to
/// be browsed as `popular` under its own heading.
#[tokio::test]
async fn a_category_browse_does_not_offer_is_refused_before_the_hub_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Arc::new(Hub::recorded());
    let ctx = context(dir.path(), &hub).await;

    for category in ["trending", "nonsense"] {
        let refused = run(&ctx, &["gglib", "model", "browse", category])
            .await
            .expect_err("a refusal");

        let said = refused.to_string();
        assert!(
            said.contains(&format!("invalid value '{category}' for '[CATEGORY]'")),
            "{said}"
        );
        assert!(
            said.contains("[possible values: popular, recent]"),
            "{said}"
        );
    }
    assert_eq!(hub.asked(), []);
}

/// `--images` browses image models, says so in its heading, and asks the
/// Hub for them; a browse without it asks for models that chat.
#[tokio::test]
async fn images_browses_image_models() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Arc::new(Hub::recorded());
    let ctx = context(dir.path(), &hub).await;

    run(&ctx, &["gglib", "model", "browse", "recent", "--images"])
        .await
        .expect("an image browse");
    run(&ctx, &["gglib", "model", "browse"])
        .await
        .expect("a browse");

    assert_eq!(hub.kinds(), [HfModelKind::Image, HfModelKind::Chat]);

    let printed = found_text(
        hub.as_ref(),
        CliBrowseCategory::Popular,
        20,
        None,
        HfModelKind::Image,
    )
    .await
    .expect("a listing");
    assert!(
        printed.starts_with("\n🏆 POPULAR GGUF Image Models:\n"),
        "{printed}"
    );
    let empty = Hub::default();
    let none = found_text(
        &empty,
        CliBrowseCategory::Recent,
        20,
        None,
        HfModelKind::Image,
    );
    assert_eq!(none.await.unwrap(), "No recent image models found.\n");
}
