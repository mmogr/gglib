#![allow(clippy::collapsible_if)]

//! User input utilities for interactive command-line prompts.
//!
//! This module provides functions for safely collecting user input
//! including strings, numbers, and confirmations. All input functions
//! handle validation and provide default values where appropriate.

use anyhow::{Context, Result};
use std::io;

/// Prompts the user for a string input
///
/// Displays a prompt message and waits for the user to enter text.
/// The input is read from stdin and returned with whitespace trimmed.
///
/// # Arguments
/// * `prompt` - The message to display to the user
///
/// # Returns
/// * `Result<String>` - The user's input as a trimmed string, or an error if reading fails
///
/// # Errors
/// Returns an error if reading from stdin fails
///
/// # Examples
/// ```rust,no_run
/// use gglib::input::prompt_string;
///
/// fn main() -> anyhow::Result<()> {
///     let name = prompt_string("Enter your name")?;
///     println!("Hello, {}!", name);
///     Ok(())
/// }
/// ```
pub fn prompt_string(prompt: &str) -> Result<String> {
    println!("{prompt}: ");

    let mut input: String = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    Ok(input.trim().to_string())
}

/// Prompts the user for a floating-point number with validation
///
/// Repeatedly prompts until the user enters a valid positive number.
/// Invalid inputs (non-numeric or negative) will show an error message
/// and prompt again.
///
/// # Arguments
/// * `prompt` - The message to display to the user
///
/// # Returns
/// * `Result<f64>` - The user's input as a positive float, or an error if reading fails
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::utils::input::prompt_float;
///
/// // Prompt for model parameter count
/// let param_count = prompt_float("Enter parameter count (in billions)")?;
/// println!("Model has {:.1}B parameters", param_count);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn prompt_float(prompt: &str) -> Result<f64> {
    loop {
        let input: String = prompt_string(prompt)?;

        match input.parse::<f64>() {
            Ok(value) if value > 0.0 => return Ok(value),
            Ok(_) => {
                eprintln!("Please enter a positive number.");
                continue;
            }
            Err(_) => {
                eprintln!("Please enter a valid number.");
                continue;
            }
        }
    }
}

/// Prompts the user for a string input with a default value
///
/// Displays a prompt message with a suggested default value. If the user
/// just presses Enter, the default value is returned.
///
/// # Arguments
/// * `prompt` - The message to display to the user
/// * `default` - Optional default value to suggest
///
/// # Returns
/// * `Result<String>` - The user's input or default value, or an error if reading fails
///
/// # Examples
/// ```rust,no_run
/// use gglib::input::prompt_string_with_default;
///
/// fn main() -> anyhow::Result<()> {
///     let name = prompt_string_with_default("Enter your name", Some("John Doe"))?;
///     println!("Hello, {}!", name);
///     Ok(())
/// }
/// ```
pub fn prompt_string_with_default(prompt: &str, default: Option<&str>) -> Result<String> {
    if let Some(default_val) = default {
        println!("{prompt} [{default_val}]: ");
    } else {
        println!("{prompt}: ");
    }

    let mut input: String = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    let trimmed: &str = input.trim();
    if trimmed.is_empty() {
        if let Some(default_val) = default {
            Ok(default_val.to_string())
        } else {
            Ok(trimmed.to_string())
        }
    } else {
        Ok(trimmed.to_string())
    }
}

/// Prompts the user for a floating-point number with validation and default value
///
/// Shows a default value and allows the user to press Enter to accept it,
/// or enter a new positive number. Invalid inputs will show an error and re-prompt.
///
/// # Arguments
/// * `prompt` - The message to display to the user
/// * `default` - Optional default value to suggest
///
/// # Returns
/// * `Result<f64>` - The user's input or default value as a positive float
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::utils::input::prompt_float_with_default;
///
/// // Prompt with a default value
/// let context_length = prompt_float_with_default(
///     "Enter context length",
///     Some(4096.0)
/// )?;
/// println!("Using context length: {}", context_length);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn prompt_float_with_default(prompt: &str, default: Option<f64>) -> Result<f64> {
    loop {
        let input: String = if let Some(default_val) = default {
            prompt_string(&format!("{prompt} [{default_val:.1}]"))?
        } else {
            prompt_string(prompt)?
        };

        if input.trim().is_empty() {
            if let Some(default_val) = default {
                return Ok(default_val);
            }
        }

        match input.parse::<f64>() {
            Ok(value) if value > 0.0 => return Ok(value),
            Ok(_) => {
                eprintln!("Please enter a positive number.");
                continue;
            }
            Err(_) => {
                eprintln!("Please enter a valid number.");
                continue;
            }
        }
    }
}

/// Prompts the user for a yes/no confirmation
///
/// Displays a confirmation prompt and waits for the user to respond
/// with 'y', 'yes', 'n', 'no', or empty (defaults to no).
///
/// # Arguments
/// * `prompt` - The question to ask the user
///
/// # Returns
/// * `Result<bool>` - true if user confirms, false otherwise
///
/// # Errors
/// Returns an error if reading from stdin fails
///
/// # Examples
/// ```rust,no_run
/// use gglib::input::prompt_confirmation;
///
/// fn main() -> anyhow::Result<()> {
///     let confirmed = prompt_confirmation("Delete this file?")?;
///     if confirmed {
///         println!("File will be deleted");
///     } else {
///         println!("Operation cancelled");
///     }
///     Ok(())
/// }
/// ```
pub fn prompt_confirmation(prompt: &str) -> Result<bool> {
    loop {
        let input = prompt_string(&format!("{prompt} (y/N)"))?;
        match input.to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" | "" => return Ok(false),
            _ => {
                eprintln!("Please enter 'y' for yes or 'n' for no.");
                continue;
            }
        }
    }
}

#[cfg(test)]
mod tests {

    // Note: Testing interactive input functions is challenging because they
    // read from stdin. In a real-world scenario, you'd want to:
    // 1. Create testable versions that accept a reader/writer
    // 2. Use dependency injection to pass in mock stdin/stdout
    // 3. Test the parsing logic separately from the input reading

    #[test]
    fn test_confirmation_parsing_logic() {
        // Test the logic that would be used in prompt_confirmation
        // if we extracted the parsing from the input reading

        let test_cases = vec![
            ("y", true),
            ("yes", true),
            ("Y", true),
            ("YES", true),
            ("n", false),
            ("no", false),
            ("N", false),
            ("NO", false),
            ("", false),
        ];

        for (input, expected) in test_cases {
            let result = match input.to_lowercase().as_str() {
                "y" | "yes" => true,
                "n" | "no" | "" => false,
                _ => continue, // Invalid input case
            };
            assert_eq!(result, expected, "Failed for input: '{}'", input);
        }
    }

    #[test]
    fn test_invalid_confirmation_inputs() {
        let invalid_inputs = vec!["maybe", "1", "true", "false", "yep", "nope"];

        for input in invalid_inputs {
            let is_valid = matches!(input.to_lowercase().as_str(), "y" | "yes" | "n" | "no" | "");
            assert!(!is_valid, "Input '{}' should be invalid", input);
        }
    }

    #[test]
    fn test_float_parsing_logic() {
        // Test float parsing that would be used in prompt_float functions
        let valid_floats = vec![
            ("1.5", 1.5),
            ("0.0", 0.0),
            ("7", 7.0),
            ("13.2", 13.2),
            ("0.1", 0.1),
        ];

        for (input, expected) in valid_floats {
            let parsed = input.parse::<f64>().unwrap();
            assert!(
                (parsed - expected).abs() < f64::EPSILON,
                "Failed for input: '{}'",
                input
            );
        }

        let invalid_floats = vec!["abc", "1.2.3", "", "text"];
        for input in invalid_floats {
            assert!(
                input.parse::<f64>().is_err(),
                "Input '{}' should fail to parse",
                input
            );
        }
    }

    #[test]
    fn test_string_trimming_logic() {
        // Test string processing that would be used in prompt_string functions
        let test_cases = vec![
            ("  hello  ", "hello"),
            ("\n\tworld\n", "world"),
            ("test", "test"),
            ("", ""),
            ("   ", ""),
        ];

        for (input, expected) in test_cases {
            let trimmed = input.trim();
            assert_eq!(trimmed, expected, "Failed for input: '{}'", input);
        }
    }

    #[test]
    fn test_default_value_logic() {
        // Test default value handling logic
        fn handle_default_string(input: &str, default: Option<&str>) -> String {
            if input.trim().is_empty() {
                default.unwrap_or("").to_string()
            } else {
                input.trim().to_string()
            }
        }

        assert_eq!(handle_default_string("", Some("default")), "default");
        assert_eq!(handle_default_string("   ", Some("default")), "default");
        assert_eq!(
            handle_default_string("user_input", Some("default")),
            "user_input"
        );
        assert_eq!(handle_default_string("", None), "");
    }

    #[test]
    fn test_float_default_logic() {
        // Test float default value handling
        fn handle_default_float(input: &str, default: Option<f64>) -> Result<f64, String> {
            if input.trim().is_empty() {
                default.ok_or_else(|| "No default provided".to_string())
            } else {
                input
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| "Invalid float".to_string())
            }
        }

        assert_eq!(handle_default_float("", Some(1.5)), Ok(1.5));
        assert_eq!(handle_default_float("7.2", Some(1.5)), Ok(7.2));
        assert!(handle_default_float("", None).is_err());
        assert!(handle_default_float("abc", Some(1.5)).is_err());
    }

    #[test]
    fn test_prompt_formatting() {
        // Test prompt message formatting logic
        fn format_prompt_with_default(prompt: &str, default: Option<&str>) -> String {
            match default {
                Some(def) => format!("{} [{}]: ", prompt, def),
                None => format!("{}: ", prompt),
            }
        }

        assert_eq!(
            format_prompt_with_default("Enter name", Some("John")),
            "Enter name [John]: "
        );
        assert_eq!(
            format_prompt_with_default("Enter value", None),
            "Enter value: "
        );
    }

    // For more comprehensive testing, you would:
    // 1. Create versions of these functions that accept std::io::Read + std::io::Write
    // 2. Test with mock stdin/stdout using std::io::Cursor
    // 3. Test error conditions (e.g., EOF, IO errors)
}
