//! Test port constants.
//!
//! Centralized port definitions for tests to prevent hardcoded values
//! and make it easier to adjust for CI environments or dynamic allocation.

/// CORS origin for unit tests (e.g., proxy origin checking)
pub const TEST_CORS_ORIGIN: &str = "http://localhost:3000";

/// Mock proxy port used in integration tests
pub const TEST_PROXY_PORT: u16 = 8080;

/// Mock llama-server base port for integration tests
pub const TEST_BASE_PORT: u16 = 19000;

/// Mock llama-server base port for path resolution tests
pub const TEST_LLAMA_BASE_PORT: u16 = 9000;
