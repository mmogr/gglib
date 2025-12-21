//! Common utilities for installation instructions.

// ANSI color codes
pub const BLUE: &str = "\x1b[34m";
pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

/// Print a section header for installation instructions.
pub fn print_header(title: &str) {
    println!(
        "\n{}{}Installation Instructions ({}):{}",
        BOLD, BLUE, title, RESET
    );
    println!("{}", "=".repeat(60));
}

/// Print a command with proper formatting.
pub fn print_command(cmd: &str) {
    println!("  {}$ {}{}", BLUE, cmd, RESET);
}

/// Print a subsection header.
pub fn print_subsection(title: &str) {
    println!("\n{}{}:{}", BOLD, title, RESET);
}
