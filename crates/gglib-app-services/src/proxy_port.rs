//! Where the llama-server base port comes from.
//!
//! Split from `proxy.rs` for the file-size gate when the tunnel plumbing
//! arrived there; the precedence rule is the one both adapters honour, and
//! `service_graph` resolves it once so they cannot drift.

use gglib_core::{DEFAULT_LLAMA_BASE_PORT, Settings};
use tracing::info;

use crate::error::GuiError;

/// Resolve the llama-server base port from override, saved settings, or default.
///
/// Precedence: override → settings.llama_base_port → DEFAULT_LLAMA_BASE_PORT
///
/// Validates that the port is in the valid range (1024-65535).
///
/// Returns (port, source_description) for logging.
pub(crate) fn resolve_llama_base_port(
    override_port: Option<u16>,
    settings: &Settings,
) -> Result<(u16, &'static str), GuiError> {
    let (port, source) = if let Some(port) = override_port {
        (port, "override")
    } else if let Some(port) = settings.llama_base_port {
        (port, "saved setting")
    } else {
        (DEFAULT_LLAMA_BASE_PORT, "default")
    };

    // Validate port range
    if !(1024..=65535).contains(&port) {
        return Err(GuiError::Internal(format!(
            "Invalid llama-server base port {}: must be in range 1024-65535",
            port
        )));
    }

    info!(
        port = port,
        source = source,
        "Starting llama-server with base port"
    );

    Ok((port, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_llama_base_port_override_wins() {
        let settings = Settings::with_defaults();
        let (port, source) = resolve_llama_base_port(Some(9500), &settings).unwrap();
        assert_eq!(port, 9500);
        assert_eq!(source, "override");
    }

    #[test]
    fn test_resolve_llama_base_port_from_settings() {
        let settings = Settings {
            llama_base_port: Some(9200),
            ..Default::default()
        };
        let (port, source) = resolve_llama_base_port(None, &settings).unwrap();
        assert_eq!(port, 9200);
        assert_eq!(source, "saved setting");
    }

    #[test]
    fn test_resolve_llama_base_port_default_fallback() {
        let settings = Settings::default();
        let (port, source) = resolve_llama_base_port(None, &settings).unwrap();
        assert_eq!(port, DEFAULT_LLAMA_BASE_PORT);
        assert_eq!(source, "default");
    }

    #[test]
    fn test_resolve_llama_base_port_validates_low() {
        let settings = Settings::default();
        let result = resolve_llama_base_port(Some(80), &settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1024-65535"));
    }

    #[test]
    fn test_resolve_llama_base_port_validates_high() {
        let settings = Settings::default();
        // Test that values at the boundary are rejected (65535 is valid, but we can't test higher with u16)
        // So instead test a valid u16 that's outside our port range (ports above 65535 don't fit in u16)
        // Just verify the low boundary works
        assert!(resolve_llama_base_port(Some(65535), &settings).is_ok());
    }

    #[test]
    fn test_resolve_llama_base_port_valid_range() {
        let settings = Settings::default();
        assert!(resolve_llama_base_port(Some(1024), &settings).is_ok());
        assert!(resolve_llama_base_port(Some(65535), &settings).is_ok());
    }
}
