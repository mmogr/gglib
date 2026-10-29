//! Add a GGUF already on disk to a library, as `gglib model add` adds one
//! to the desktop's, and print the name it is served under.
//!
//! ```text
//! gglib-ffi-add-model <data-dir> <models-dir> <gguf>
//! ```
//!
//! `<data-dir>` and `<models-dir>` are a library's folders, as the app hands
//! them to `open_library`. The file stays where it is. The Swift smoke test
//! (`scripts/apple/swift-smoke.sh`) runs this before it serves the model
//! through the binding, which has no call that adds a file: a phone gets its
//! models by download. Behind the crate's `smoke` feature; not shipped.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
use gglib_core::ports::NoopEmitter;
use gglib_core::services::ImportMode;
use gglib_ffi::DATABASE_FILE;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, models_dir, gguf] = args.as_slice() else {
        eprintln!("usage: gglib-ffi-add-model <data-dir> <models-dir> <gguf>");
        return ExitCode::from(2);
    };
    let added = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
        .and_then(|runtime| {
            runtime.block_on(add(
                Path::new(data_dir),
                PathBuf::from(models_dir),
                Path::new(gguf),
            ))
        });
    match added {
        Ok(name) => {
            println!("{name}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Add `gguf` to the library in `data_dir` and `models_dir`; its name.
async fn add(data_dir: &Path, models_dir: PathBuf, gguf: &Path) -> Result<String, String> {
    let config = BootstrapConfig {
        models_dir: Some(models_dir),
        ..BootstrapConfig::for_db(data_dir.join(DATABASE_FILE))
    };
    let core = CoreBootstrap::build(config, Arc::new(NoopEmitter))
        .await
        .map_err(|e| e.to_string())?;
    let model = core
        .app
        .models()
        .import_from_file(gguf, core.gguf_parser.as_ref(), None, ImportMode::Fresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(model.name)
}
