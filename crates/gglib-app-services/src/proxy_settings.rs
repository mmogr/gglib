//! The proxy config the settings amount to: what a start that names nothing
//! gets, on every surface.
//!
//! The desktop app's Turn on, the tray, `proxy_autostart`, a model start
//! that brings the endpoint up, and `gglib proxy` with no flags all start
//! the endpoint without saying where or how. This is the one place the
//! stored answers become a config, so none of them reads a setting the
//! others do not.

use gglib_core::Settings;
use gglib_runtime::proxy::{ProxyConfig, resolve_disk_budget};

/// The config a start that names nothing gets.
///
/// The stored address, port, allowed hosts and prompt-cache choice, the
/// global default context passed through unresolved, and the rest of
/// [`ProxyConfig::default`].
#[must_use]
pub fn proxy_config_from_settings(settings: &Settings) -> ProxyConfig {
    let cache_enabled = settings.effective_proxy_cache_on_disk();
    ProxyConfig {
        host: settings.effective_proxy_host(),
        port: settings.effective_proxy_port(),
        // Passed through, not resolved: resolving here turned "the user set
        // nothing" into "the user set 4096", which is what made every rung
        // below the global default unreachable.
        default_context: settings.default_context_size,
        cache_enabled,
        slot_dir: cache_enabled.then(gglib_runtime::default_slot_dir),
        disk_budget: resolve_disk_budget(settings.proxy_cache_disk_gb),
        allowed_hosts: settings.effective_proxy_allowed_hosts(),
        ..ProxyConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing stored is loopback on the default port, no allowed hosts and
    /// no cache on disk: what every start got before the settings existed.
    #[test]
    fn nothing_stored_is_the_loopback_endpoint_with_no_cache() {
        let config = proxy_config_from_settings(&Settings::default());
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, gglib_core::settings::DEFAULT_PROXY_PORT);
        assert!(config.allowed_hosts.is_empty());
        assert!(!config.cache_enabled);
        assert_eq!(config.slot_dir, None);
        assert_eq!(config.default_context, None);
    }

    /// Each stored answer reaches the config: the address, the hosts, the
    /// cache with its default folder and its budget.
    ///
    /// The data root is this process's own before either read of the slot
    /// folder: another test in this binary isolating it between the two
    /// reads would move it from the user's folder to the test one.
    #[test]
    fn each_stored_answer_reaches_the_config() {
        gglib_core::paths::isolate_data_root();
        let settings = Settings {
            proxy_host: Some("0.0.0.0".to_owned()),
            proxy_port: Some(8123),
            proxy_allowed_hosts: Some(vec!["gglib.lan".to_owned()]),
            proxy_cache_on_disk: Some(true),
            proxy_cache_disk_gb: Some(4),
            ..Settings::default()
        };
        let config = proxy_config_from_settings(&settings);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8123);
        assert_eq!(config.allowed_hosts, ["gglib.lan"]);
        assert!(config.cache_enabled);
        assert_eq!(config.slot_dir, Some(gglib_runtime::default_slot_dir()));
        assert_eq!(config.disk_budget, resolve_disk_budget(Some(4)));
    }
}
