//! What `gglib config models-dir show` prints: the folder, how it was
//! chosen, and its state in the word every surface uses.

use gglib_app_services::types::ModelsDirectoryInfo;
use gglib_core::domain::ModelsDirState;

use super::models_dir_lines;

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
    assert_eq!(
        models_dir_lines(&folder(true, true)),
        [
            "Current models directory: /data/models (source: environment)",
            "Status: Writable",
        ]
    );
    assert_eq!(
        models_dir_lines(&folder(false, true))[1],
        "Status: Will be created"
    );
    assert_eq!(
        models_dir_lines(&folder(true, false))[1],
        "Status: Not writable"
    );
    assert_eq!(
        models_dir_lines(&folder(false, false))[1],
        "Status: Not writable"
    );
}
