//! Configuration types for [`crate::CoreBootstrap::build`].

use std::fmt;
use std::path::PathBuf;

/// Configuration required to run [`crate::CoreBootstrap::build`].
///
/// The database path must be fully resolved by the caller before passing
/// this struct. The helper that resolves it (`database_path`) is deliberately
/// kept in `gglib-core::paths` so that adapters own their own path
/// strategies.
///
/// [`BootstrapConfig::for_db`] is the daemon's and the CLI's: no models
/// directory and no token of their own. A caller that keeps its models in a
/// folder it owns and its token where it chooses, as an app on a phone does,
/// names both.
#[derive(Clone)]
pub struct BootstrapConfig {
    /// Absolute path to the `SQLite` database file.
    pub db_path: PathBuf,
    /// The directory the download manager puts every download under,
    /// whatever is stored. A caller that resolves the directory itself, as
    /// the app services' model upgrade does, does not see it.
    ///
    /// With `None` the download manager is handed no directory and asks
    /// `resolve_models_dir` as each download starts, so a running daemon's
    /// download goes where the directory resolves then, not where it
    /// resolved as the daemon started.
    pub models_dir: Option<PathBuf>,
    /// The `HuggingFace` token the Hub client, the download manager and
    /// `AppCore` are handed. It is a secret, and this config's `Debug` says
    /// only whether it holds one.
    ///
    /// It is read by the rule `HF_TOKEN`'s value is,
    /// `gglib_core::hf_token::normalised`: the spaces around it are dropped,
    /// and a blank one is no token. With `None`, or a blank one,
    /// [`crate::CoreBootstrap::build`] reads the one in the environment.
    pub hf_token: Option<String>,
}

impl BootstrapConfig {
    /// The database at `db_path`, with no models directory and no token: the
    /// download manager asks for the directory as each download starts, and
    /// the token is the environment's.
    #[must_use]
    pub const fn for_db(db_path: PathBuf) -> Self {
        Self {
            db_path,
            models_dir: None,
            hf_token: None,
        }
    }
}

impl fmt::Debug for BootstrapConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BootstrapConfig")
            .field("db_path", &self.db_path)
            .field("models_dir", &self.models_dir)
            .field("has_hf_token", &self.hf_token.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a token of any account.
    const FAKE_TOKEN: &str = "hf_fake_token_for_a_test";

    #[test]
    fn a_config_for_a_database_names_no_models_directory_and_no_token() {
        let config = BootstrapConfig::for_db(PathBuf::from("/data/gglib.db"));

        assert_eq!(config.db_path, PathBuf::from("/data/gglib.db"));
        assert_eq!(config.models_dir, None);
        assert_eq!(config.hf_token, None);
    }

    #[test]
    fn a_configs_debug_says_it_holds_a_token_but_not_what_it_is() {
        let config = BootstrapConfig {
            hf_token: Some(FAKE_TOKEN.to_owned()),
            ..BootstrapConfig::for_db(PathBuf::from("/data/gglib.db"))
        };

        let shown = format!("{config:?}");

        assert!(!shown.contains(FAKE_TOKEN), "{shown}");
        assert!(shown.contains("has_hf_token: true"), "{shown}");
    }
}
