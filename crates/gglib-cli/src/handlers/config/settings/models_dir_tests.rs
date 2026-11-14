//! The row `gglib config show` prints for `models-dir`: the folder, how it
//! was chosen, and its state in the word every surface uses.

use gglib_app_services::types::ModelsDirectoryInfo;
use gglib_core::domain::ModelsDirState;

use super::models_dir::row;

fn folder(exists: bool, writable: bool) -> ModelsDirectoryInfo {
    ModelsDirectoryInfo {
        path: "/data/models".to_owned(),
        source: "environment".to_owned(),
        default_path: "/home/me/.local/share/gglib/models".to_owned(),
        exists,
        writable,
        state: ModelsDirState::of(exists, writable),
    }
}

#[test]
fn the_folder_is_shown_with_its_source_and_its_state() {
    let shown = |exists, writable| row(&folder(exists, writable));
    assert_eq!(
        shown(true, true),
        (
            "models-dir".to_owned(),
            "/data/models (environment, Writable)".to_owned()
        )
    );
    assert_eq!(
        shown(false, true).1,
        "/data/models (environment, Will be created)"
    );
    assert_eq!(
        shown(true, false).1,
        "/data/models (environment, Not writable)"
    );
    assert_eq!(
        shown(false, false).1,
        "/data/models (environment, Not writable)"
    );
}
