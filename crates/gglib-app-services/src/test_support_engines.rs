//! The engines under this test binary's data root, which all its tests share.
//!
//! [`gglib_core::paths::isolate_data_root`] gives the process one data root,
//! and the code under test finds each engine's binary there with no path
//! passed in (`sd_status`, `pin_state`, `engine::uninstall`), so every test
//! in this binary that writes a stand-in engine, or reads a status that runs
//! one, meets the same files. Windows will not rewrite a file while a status
//! is running it (#2045). Such a test holds [`Engines`] for its whole body.

use std::path::PathBuf;

use gglib_core::domain::RuntimeKind;

/// This binary's engines, held by one test at a time.
pub(crate) struct Engines {
    _held: (),
}

impl Engines {
    /// Hold the engines, once no other test does.
    pub(crate) async fn hold() -> Self {
        gglib_core::paths::isolate_data_root();
        Self { _held: () }
    }

    /// [`Self::hold`], from a thread outside any runtime.
    pub(crate) fn hold_blocking() -> Self {
        gglib_core::paths::isolate_data_root();
        Self { _held: () }
    }

    /// A file where `kind`'s server goes under this binary's data root, as an
    /// install leaves one as far as a status or a removal can tell, and its
    /// path. It is not a program: nothing these tests ask needs it to run.
    pub(crate) fn stand_in(&self, kind: RuntimeKind) -> PathBuf {
        let root = gglib_core::paths::isolate_data_root();
        let binary = match kind {
            RuntimeKind::Llama => gglib_core::paths::llama_server_path(),
            RuntimeKind::StableDiffusion => gglib_core::paths::sd_server_path(),
        }
        .expect("the engine's path resolves");
        assert!(binary.starts_with(root), "{}", binary.display());
        std::fs::create_dir_all(binary.parent().expect("a bin folder"))
            .expect("the engine's bin folder");
        std::fs::write(&binary, b"not a program\n").expect("the stand-in is written");
        binary
    }
}
