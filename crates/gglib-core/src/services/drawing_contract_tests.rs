//! `contracts/images/drawing.json`: the availability answers a client that
//! offers a choice of image model replays, as the rule itself gives them
//! over this module's test library.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::image_model_answer;
use super::model_tests::{FLUX, SDXL, TURBO, library};
use crate::contracts::http::images::DrawingAvailability;

/// The recorded answers, by name. The field order is the file's order.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recorded {
    /// No model chosen, a default set: it draws, and is marked on its row.
    unnamed: DrawingAvailability,
    /// `?image_model=` naming a complete model that is not the default.
    chosen: DrawingAvailability,
    /// `?image_model=` naming one that lacks files.
    chosen_incomplete: DrawingAvailability,
    /// `?image_model=` naming an id no model here has.
    chosen_gone: DrawingAvailability,
    /// No model chosen, several complete and no default set.
    several_and_no_default: DrawingAvailability,
}

async fn recorded() -> Recorded {
    let default = Some(i64::from(TURBO));
    let library = library();
    Recorded {
        unnamed: image_model_answer(&library, default, None, &[SDXL]).await,
        chosen: image_model_answer(&library, default, Some(SDXL), &[SDXL]).await,
        chosen_incomplete: image_model_answer(&library, default, Some(FLUX), &[SDXL]).await,
        chosen_gone: image_model_answer(&library, default, Some(99), &[SDXL]).await,
        several_and_no_default: image_model_answer(&library, None, None, &[]).await,
    }
}

fn recorded_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/images/drawing.json")
}

/// The checked-in file is exactly what the rule answers. Run with
/// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
#[tokio::test]
async fn recorded_answers_match_the_checked_in_file() {
    let mut want = serde_json::to_string_pretty(&recorded().await).expect("serialise");
    want.push('\n');
    let path = recorded_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("make contracts/images");
        std::fs::write(&path, &want).expect("write drawing.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/images/drawing.json");
    assert!(
        have == want,
        "contracts/images/drawing.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}

/// The file reads back to the same answers, and every one of them lists
/// the image models, drawable or not.
#[tokio::test]
async fn every_recorded_answer_decodes_back_and_lists_the_models() {
    let file = std::fs::read_to_string(recorded_path()).expect("read drawing.json");
    let decoded: Recorded = serde_json::from_str(&file).expect("decode drawing.json");
    assert_eq!(decoded, recorded().await);
    let raw: serde_json::Value = serde_json::from_str(&file).unwrap();
    for (name, answer) in raw.as_object().unwrap() {
        assert_eq!(answer["models"].as_array().map(Vec::len), Some(3), "{name}");
    }
}
