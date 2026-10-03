//! `--mmproj` on the llama-server command line.

use std::path::{Path, PathBuf};

use gglib_core::ports::ServerConfig;

use super::build_command;

fn args(config: &ServerConfig) -> Vec<String> {
    build_command(Path::new("/fake/llama-server"), config, 5500)
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn config() -> ServerConfig {
    ServerConfig::new(
        1,
        "test-model".to_owned(),
        PathBuf::from("/tmp/test.gguf"),
        9000,
    )
}

#[test]
fn a_projector_is_passed_as_mmproj_with_its_path() {
    let projector = PathBuf::from("/models/X.mmproj-Q8_0.gguf");
    let args = args(&config().with_mmproj(Some(projector)));

    let flag = args
        .iter()
        .position(|arg| arg == "--mmproj")
        .unwrap_or_else(|| panic!("--mmproj missing from {args:?}"));
    assert_eq!(args[flag + 1], "/models/X.mmproj-Q8_0.gguf");
    assert_eq!(args.iter().filter(|arg| *arg == "--mmproj").count(), 1);
}

#[test]
fn no_projector_passes_no_mmproj() {
    let args = args(&config());
    assert!(!args.iter().any(|arg| arg == "--mmproj"), "got {args:?}");
    assert!(config().mmproj.is_none());

    let cleared = config()
        .with_mmproj(Some(PathBuf::from("/models/mmproj-F16.gguf")))
        .with_mmproj(None);
    assert!(
        !self::args(&cleared).iter().any(|arg| arg == "--mmproj"),
        "a cleared projector passes no flag"
    );
}
