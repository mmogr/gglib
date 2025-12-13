//! Llama.cpp binary path resolution.
//!
//! Provides paths to the managed llama-server and llama-cli binaries,
//! as well as the llama.cpp repository and configuration files.

use std::path::PathBuf;

use super::error::PathError;
use super::platform::resource_root;

/// Get the gglib data directory containing llama binaries.
///
/// Returns the `.llama/` directory containing helper binaries.
/// In dev, this is in the repo. In release, this is in the user data dir.
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

/// Get the path to the managed llama-cli binary.
pub fn llama_cli_path() -> Result<PathBuf, PathError> {
    let gglib_dir = gglib_data_dir()?;

    #[cfg(target_os = "windows")]
    let binary_name = "llama-cli.exe";

    #[cfg(not(target_os = "windows"))]
    let binary_name = "llama-cli";

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

    #[test]
    fn test_llama_server_path() {
        let result = llama_server_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        #[cfg(target_os = "windows")]
        assert!(path.to_string_lossy().ends_with("llama-server.exe"));

        #[cfg(not(target_os = "windows"))]
        assert!(path.to_string_lossy().ends_with("llama-server"));
    }

    #[test]
    fn test_llama_cli_path() {
        let result = llama_cli_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        #[cfg(target_os = "windows")]
        assert!(path.to_string_lossy().ends_with("llama-cli.exe"));

        #[cfg(not(target_os = "windows"))]
        assert!(path.to_string_lossy().ends_with("llama-cli"));
    }
}
