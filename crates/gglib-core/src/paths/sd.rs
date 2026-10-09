//! stable-diffusion.cpp binary path resolution.
//!
//! Everything gglib installs for image generation lives under one directory
//! of its own, `.sd/`, beside llama.cpp's `.llama/`: the binary and the
//! shared library it loads, the install's record, a source checkout, and the
//! archive while it is unpacked. Removing that directory uninstalls it.

use std::path::PathBuf;

use super::error::PathError;
use super::platform::resource_root;

/// The command that installs stable-diffusion.cpp's `sd-server`, for a
/// message that tells the user to run it.
pub const SD_INSTALL_COMMAND: &str = "gglib config sd install";

/// The `.sd/` directory: everything the image runtime install owns.
///
/// In dev, this is in the repo. In release, this is in the user data dir.
pub fn sd_data_dir() -> Result<PathBuf, PathError> {
    Ok(resource_root()?.join(".sd"))
}

/// The managed `sd-server` binary. Its shared library is unpacked beside it,
/// where the binary's own search path looks.
pub fn sd_server_path() -> Result<PathBuf, PathError> {
    #[cfg(target_os = "windows")]
    let binary_name = "sd-server.exe";

    #[cfg(not(target_os = "windows"))]
    let binary_name = "sd-server";

    Ok(sd_data_dir()?.join("bin").join(binary_name))
}

/// The record of how the installed `sd-server` got there.
pub fn sd_config_path() -> Result<PathBuf, PathError> {
    Ok(sd_data_dir()?.join("sd-config.json"))
}

/// The stable-diffusion.cpp checkout a source build compiles.
pub fn sd_cpp_dir() -> Result<PathBuf, PathError> {
    Ok(sd_data_dir()?.join("stable-diffusion.cpp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_the_image_runtime_installs_is_under_its_own_directory() {
        let dir = sd_data_dir().unwrap();
        assert_eq!(dir.file_name().and_then(|n| n.to_str()), Some(".sd"));

        let server = sd_server_path().unwrap();
        assert_eq!(server.parent(), Some(dir.join("bin").as_path()));
        #[cfg(target_os = "windows")]
        assert!(server.ends_with("sd-server.exe"));
        #[cfg(not(target_os = "windows"))]
        assert!(server.ends_with("sd-server"));

        assert_eq!(sd_config_path().unwrap(), dir.join("sd-config.json"));
        assert_eq!(sd_cpp_dir().unwrap(), dir.join("stable-diffusion.cpp"));
    }
}
