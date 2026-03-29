//! Common utilities for installation instructions.

// Re-export from the centralised style module so sibling instruction files
// can continue to use `super::common::{BOLD, RESET}`.
pub use crate::presentation::style::{BOLD, INFO, RESET};

/// Print a section header for installation instructions.
pub fn print_header(title: &str) {
    println!(
        "\n{}{}Installation Instructions ({}):{}",
        BOLD, INFO, title, RESET
    );
    println!("{}", "=".repeat(60));
}

/// Print a command with proper formatting.
pub fn print_command(cmd: &str) {
    println!("  {}$ {}{}", INFO, cmd, RESET);
}

/// Print a subsection header.
pub fn print_subsection(title: &str) {
    println!("\n{}{}:{}", BOLD, title, RESET);
}
