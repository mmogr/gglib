#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]

pub mod factory;
pub mod repositories;
pub mod setup;

// Re-export factory for convenient access
pub use factory::CoreFactory;

// Re-export TestDb for integration tests
#[cfg(any(test, feature = "test-utils"))]
pub use factory::TestDb;

// Re-export repository implementations
pub use repositories::{
    SqliteChatHistoryRepository, SqliteDownloadStateRepository, SqliteMcpRepository,
    SqliteModelRepository, SqliteSettingsRepository,
};

// Re-export setup functions for convenient access
#[cfg(any(test, feature = "test-utils"))]
pub use setup::setup_test_database;
pub use setup::{ensure_column_exists, setup_database};
