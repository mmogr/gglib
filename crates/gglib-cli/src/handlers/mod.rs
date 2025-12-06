//! Command handlers that delegate to AppCore.
//!
//! This module will contain the actual command execution logic once migrated.
//! Currently, handlers are re-exported from the root gglib crate.
//!
//! Handlers are thin wrappers that:
//! 1. Parse/validate CLI-specific input
//! 2. Call AppCore methods
//! 3. Format output for the terminal
//!
//! Handlers should NOT:
//! - Access repositories directly
//! - Contain business logic
//! - Manage database connections

// Re-export handlers from root crate for now
// TODO: Migrate these handlers here from src/commands/
pub use gglib::commands::add::handle_add;
pub use gglib::commands::list::handle_list;

