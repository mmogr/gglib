//! Test port constants for gglib-axum tests.
//!
//! Centralized port definitions to prevent hardcoded values.

/// CORS origin for unit tests (e.g., proxy origin checking)
pub const TEST_CORS_ORIGIN: &str = "http://localhost:3000";

/// Mock proxy port used in integration tests
pub const TEST_MODEL_PORT: u16 = 8080;

/// Mock llama-server base port for test configurations
pub const TEST_BASE_PORT: u16 = 19000;
