//! Llama.cpp binary path resolution.
//!
//! Provides paths to the managed llama-server binary,
//! as well as the llama.cpp repository and configuration files.

use std::path::PathBuf;

use super::error::PathError;
use super::platform::resource_root;

/// The command that installs llama.cpp, for a message that tells the user to
/// run it.
pub const LLAMA_INSTALL_COMMAND: &str = "gglib config llama install";

/// Get the gglib data directory containing llama binaries.
///
/// Returns the `.llama/` directory containing helper binaries.
/// Under the resource root, which is the data root unless `GGLIB_RESOURCE_DIR` names another.
pub fn gglib_data_dir() -> Result<PathBuf, PathError> {
    Ok(resource_root()?.join(".llama"))
}

/// Get the path to the managed llama-server binary.
pub fn llama_server_path() -> Result<PathBuf, PathError> {
    let gglib_dir = gglib_data_dir()?;

    #[cfg(target_os = "windows")]
    let binary_name = "llama-server.exe";

    #[cfg(not(target_os = "windows"))]
    let binary_name = "llama-server";

    Ok(gglib_dir.join("bin").join(binary_name))
}

/// Get the path to the managed `llama-bench` binary.
///
/// `llama-bench` is included in the pre-built binary archives alongside
/// `llama-server`. For source builds it requires `-DLLAMA_BUILD_BENCH=ON`
/// (see Phase 6 of the benchmark implementation plan).
pub fn llama_bench_path() -> Result<PathBuf, PathError> {
    let gglib_dir = gglib_data_dir()?;

    #[cfg(target_os = "windows")]
    let binary_name = "llama-bench.exe";

    #[cfg(not(target_os = "windows"))]
    let binary_name = "llama-bench";

    Ok(gglib_dir.join("bin").join(binary_name))
}

/// Get the path to the llama.cpp repository directory.
pub fn llama_cpp_dir() -> Result<PathBuf, PathError> {
    let gglib_dir = gglib_data_dir()?;
    Ok(gglib_dir.join("llama.cpp"))
}

/// Get the path to the llama build configuration file.
pub fn llama_config_path() -> Result<PathBuf, PathError> {
    let gglib_dir = gglib_data_dir()?;
    Ok(gglib_dir.join("llama-config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};

    /// A data root of the test's own, under the lock: these resolve through
    /// the data root, which is otherwise the user's real one. In the order a
    /// tuple drops them: the variable is restored before the lock is let go.
    fn own_root() -> (
        EnvVarGuard,
        tempfile::TempDir,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().expect("tempdir");
        let env = EnvVarGuard::set("GGLIB_DATA_DIR", root.path().to_string_lossy().as_ref());
        (env, root, guard)
    }

    #[test]
    fn test_llama_server_path() {
        let _root = own_root();
        let result = llama_server_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        #[cfg(target_os = "windows")]
        assert!(path.to_string_lossy().ends_with("llama-server.exe"));

        #[cfg(not(target_os = "windows"))]
        assert!(path.to_string_lossy().ends_with("llama-server"));
    }

    #[test]
    fn test_llama_bench_path() {
        let _root = own_root();
        let result = llama_bench_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        #[cfg(target_os = "windows")]
        assert!(path.to_string_lossy().ends_with("llama-bench.exe"));

        #[cfg(not(target_os = "windows"))]
        assert!(path.to_string_lossy().ends_with("llama-bench"));
    }
}
