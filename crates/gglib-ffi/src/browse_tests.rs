//! The shortlist, a search and a repository's quantizations, through a
//! library over a Hub that answers with gglib-hf's recordings.

use gglib_core::domain::fit_verdict;
use gglib_core::ports::HfPortError;
use tempfile::TempDir;

use super::*;
use crate::test_gguf::{CHAT_MODEL_KV, Gguf};
use crate::test_hub::{HubFile, RECORDED_REPO, RECORDED_SIZES, RECORDED_TRAINED_CTX, TestHub};
use crate::test_library::{PHONE, library_over, wait};
use crate::verdict::carried;

/// gglib-core's verdict on the recorded model at `weights` bytes, on
/// [`PHONE`].
fn core_verdict(weights: u64) -> gglib_core::domain::FitVerdict {
    fit_verdict(
        Some(u64::from(RECORDED_TRAINED_CTX)),
        Some(weights),
        Some(CHAT_MODEL_KV),
        PHONE.budget(),
    )
    .0
}

#[test]
fn the_shortlist_is_suggested_row_for_row_with_its_verdicts_and_reasons() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);

    let suggested = library.suggested(PHONE);

    assert_eq!(suggested.len(), PHONE_SHORTLIST.len());
    for (suggestion, row) in suggested.iter().zip(PHONE_SHORTLIST) {
        assert_eq!(suggestion.repo, row.repo);
        assert_eq!(suggestion.quantization, row.quantization);
        assert_eq!(suggestion.bytes, row.weights_bytes);
        assert_eq!(suggestion.rationale, row.rationale);
        assert_eq!(suggestion.why.trained_ctx, Some(row.trained_ctx));
        assert_eq!(suggestion.why.budget_bytes, PHONE.budget());
        assert!(
            matches!(suggestion.verdict, FitVerdict::Fits { .. }),
            "an 8 GiB phone fits every row of its shortlist: {suggestion:?}"
        );
    }
}

#[test]
fn a_search_is_the_hubs_page_of_repositories() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);

    let page = wait(library.search("phi-4 mini".to_owned(), None)).expect("a page");

    let repos: Vec<&str> = page.hits.iter().map(|hit| hit.repo.as_str()).collect();
    assert_eq!(
        repos,
        [
            "MaziyarPanahi/Phi-4-mini-instruct-GGUF",
            RECORDED_REPO,
            "bartowski/microsoft_Phi-4-mini-instruct-GGUF",
        ]
    );
    let unsloth = &page.hits[1];
    assert_eq!((unsloth.likes, unsloth.downloads), (153, 117_114));
    assert_eq!(unsloth.trained_ctx, Some(131_072));
    assert!(
        unsloth
            .parameters_b
            .is_some_and(|b| (b - 3.836).abs() < 0.001)
    );
    assert_eq!(page.next_cursor.as_deref(), Some("next"));
}

/// Each quantization is judged from its own head at its own size, so one
/// repository runs from fits to too big.
#[test]
fn each_quantization_is_judged_from_its_head_at_its_size() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);

    let offers =
        wait(library.quantizations(RECORDED_REPO.to_owned(), PHONE)).expect("the quantizations");

    let mut names: Vec<&str> = offers.iter().map(|offer| offer.name.as_str()).collect();
    names.sort_unstable();
    let mut recorded: Vec<&str> = RECORDED_SIZES.iter().map(|(name, _)| *name).collect();
    recorded.sort_unstable();
    assert_eq!(names, recorded);
    for offer in &offers {
        let expected = carried(core_verdict(offer.total_bytes), false);
        assert_eq!(offer.verdict, expected, "{}", offer.name);
        assert_eq!(offer.why.trained_ctx, Some(u64::from(RECORDED_TRAINED_CTX)));
        assert_eq!(offer.why.weights_bytes, Some(offer.total_bytes));
        assert_eq!(offer.shard_count, 1);
    }
    let verdict_of = |name: &str| {
        offers
            .iter()
            .find(|offer| offer.name == name)
            .unwrap()
            .verdict
    };
    assert!(matches!(verdict_of("Q2_K"), FitVerdict::Fits { .. }));
    assert!(matches!(
        verdict_of("Q8_0"),
        FitVerdict::Tight { ctx: 4096 }
    ));
    assert!(matches!(verdict_of("BF16"), FitVerdict::TooBig { .. }));
}

/// A file whose tokenizer comes first, as llama.cpp's own small models'
/// does, has its fit keys past 64 KiB: they are cut off, not missing.
#[test]
fn a_quantization_whose_fit_keys_lie_past_the_head_cannot_be_told_yet() {
    let root = TempDir::new().unwrap();
    let tokenizer_first = Gguf::new()
        .text("general.architecture", "llama")
        .tokens(20_000)
        .u32("llama.context_length", 4096)
        .bytes();
    let hub = TestHub {
        files: vec![HubFile {
            repo: "tester/tokenizer-first-GGUF".to_owned(),
            path: "tokenizer-first-Q8_0.gguf".to_owned(),
            bytes: tokenizer_first,
        }],
        ..TestHub::default()
    };
    let library = library_over(hub, &root);

    let offers =
        wait(library.quantizations("tester/tokenizer-first-GGUF".to_owned(), PHONE)).unwrap();

    assert_eq!(
        offers[0].verdict,
        FitVerdict::Unknown {
            missing: MissingInput::TrainedContext,
            cut_off: true
        }
    );
}

/// A repository that wants a token is refused, in the app's words.
#[test]
fn a_hub_that_answers_401_is_auth_required() {
    let root = TempDir::new().unwrap();
    let hub = TestHub {
        refusal: Some(|| HfPortError::AuthRequired {
            model_id: RECORDED_REPO.to_owned(),
        }),
        ..TestHub::default()
    };
    let library = library_over(hub, &root);

    let offers = wait(Arc::clone(&library).quantizations(RECORDED_REPO.to_owned(), PHONE));
    let page = wait(library.search("phi".to_owned(), None));

    assert_eq!(offers, Err(GglibError::AuthRequired));
    assert_eq!(page, Err(GglibError::AuthRequired));
}
