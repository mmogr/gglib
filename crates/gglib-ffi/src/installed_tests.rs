//! The installed list, and removing a model: files, row, and the folder it
//! leaves empty, but never a file another model names.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use gglib_core::domain::{NewModel, NewModelFile};
use tempfile::TempDir;

use super::*;
use crate::GglibError;
use crate::library::Library;
use crate::test_hub::TestHub;
use crate::test_library::{PHONE, dirs_in, library_over, wait};
use crate::verdict::MissingInput;

const MIB: usize = 1 << 20;

/// A file of `len` bytes at `path`, its folders made.
fn file(path: &Path, len: usize) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![7_u8; len]).unwrap();
    path.to_path_buf()
}

/// A llama-shaped model at `weights`, trained for 40,960 tokens, with
/// `tags`, stored in `library`. Answers its id.
fn store(library: &Library, weights: &Path, projector: Option<&Path>, tags: &[&str]) -> i64 {
    let mut model = NewModel::new(
        "Test model".to_owned(),
        weights.to_path_buf(),
        1.0,
        Utc::now(),
    );
    model.architecture = Some("llama".to_owned());
    model.context_length = Some(40_960);
    model.projector_path = projector.map(Path::to_path_buf);
    model.tags = tags.iter().map(|tag| (*tag).to_owned()).collect();
    model.metadata = HashMap::from(
        [
            ("llama.block_count", "16"),
            ("llama.attention.head_count", "32"),
            ("llama.attention.head_count_kv", "8"),
            ("llama.attention.key_length", "64"),
            ("llama.attention.value_length", "64"),
        ]
        .map(|(key, value)| (key.to_owned(), value.to_owned())),
    );
    wait(library.models.insert(&model))
        .expect("the model is stored")
        .id
}

/// A row for each of `files`, as a download records them: by their path in
/// the repository's folder.
fn record(library: &Library, id: i64, files: &[&str]) {
    for (index, path) in files.iter().enumerate() {
        let index = i32::try_from(index).unwrap();
        let row = NewModelFile::new(id, (*path).to_owned(), index, 0, None);
        wait(library.model_files.insert(&row)).expect("the row is stored");
    }
}

fn models_dir(root: &TempDir) -> PathBuf {
    PathBuf::from(dirs_in(root).models_dir)
}

#[test]
fn an_installed_model_says_its_size_context_and_what_it_can_do() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let weights = file(&models_dir(&root).join("o_r/model.Q4_K_M.gguf"), MIB);
    let id = store(&library, &weights, None, &["reasoning", "agent"]);

    let installed = wait(Arc::clone(&library).installed()).unwrap();

    assert_eq!(installed.len(), 1);
    let model = &installed[0];
    assert_eq!((model.id, model.name.as_str()), (id, "Test model"));
    assert_eq!(model.bytes, MIB as u64);
    assert_eq!(model.trained_ctx, Some(40_960));
    assert_eq!(model.capabilities, ["reasoning"]);
}

/// With no reading handed over there is no budget to judge at; with one,
/// the model is judged at it.
#[test]
fn an_installed_model_is_judged_at_the_reading_the_library_holds() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let weights = file(&models_dir(&root).join("o_r/model.Q4_K_M.gguf"), MIB);
    store(&library, &weights, None, &[]);

    let before = wait(Arc::clone(&library).installed()).unwrap();
    let _ = library.suggested(PHONE);
    let after = wait(Arc::clone(&library).installed()).unwrap();

    assert_eq!(
        before[0].verdict,
        FitVerdict::Unknown {
            missing: MissingInput::Budget,
            cut_off: false
        }
    );
    assert!(
        matches!(after[0].verdict, FitVerdict::Fits { .. }),
        "{:?}",
        after[0]
    );
}

#[test]
fn removing_a_model_deletes_every_shard_its_row_and_the_folder_it_emptied() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let folder = models_dir(&root).join("o_r");
    let shards = ["Q8_0/X-00001-of-00002.gguf", "Q8_0/X-00002-of-00002.gguf"];
    let paths: Vec<PathBuf> = shards
        .iter()
        .map(|shard| file(&folder.join(shard), 1024))
        .collect();
    let id = store(&library, &paths[0], None, &[]);
    record(&library, id, &shards);

    wait(Arc::clone(&library).remove(id)).expect("the model is removed");

    assert!(paths.iter().all(|path| !path.exists()), "{paths:?}");
    assert!(!folder.exists(), "the emptied folder is left");
    assert!(models_dir(&root).is_dir(), "the models folder itself stays");
    assert_eq!(wait(Arc::clone(&library).installed()), Ok(Vec::new()));
}

/// Two quantizations of a repository share its projector: removing one
/// keeps it, and the folder it is in.
#[test]
fn removing_a_model_keeps_a_file_another_model_names() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);
    let folder = models_dir(&root).join("o_r");
    let projector = file(&folder.join("mmproj-F16.gguf"), 512);
    let q4 = file(&folder.join("X.Q4_K_M.gguf"), 1024);
    let q8 = file(&folder.join("X.Q8_0.gguf"), 2048);
    let first = store(&library, &q4, Some(&projector), &[]);
    let second = store(&library, &q8, Some(&projector), &[]);

    wait(Arc::clone(&library).remove(first)).unwrap();

    assert!(!q4.exists());
    assert!(projector.exists() && q8.exists());
    let left = wait(Arc::clone(&library).installed()).unwrap();
    assert_eq!(
        left.iter().map(|model| model.id).collect::<Vec<_>>(),
        [second]
    );
}

#[test]
fn removing_a_model_the_library_does_not_hold_is_not_found() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);

    assert_eq!(wait(library.remove(404)), Err(GglibError::NotFound));
}
