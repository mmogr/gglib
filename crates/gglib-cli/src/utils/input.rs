//! User input utilities for interactive command-line prompts.
//!
//! This module provides functions for safely collecting user input
//! including strings and confirmations.

use anyhow::{Context, Result};
use std::io;

/// Prompts the user for a string input.
///
/// Displays a prompt message and waits for the user to enter text.
/// The input is read from stdin and returned with whitespace trimmed.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
///
/// # Returns
///
/// * `Result<String>` - The user's input as a trimmed string
///
/// # Errors
///
/// Returns an error if reading from stdin fails.
pub fn prompt_string(prompt: &str) -> Result<String> {
    println!("{prompt}: ");

    let mut input: String = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    Ok(input.trim().to_string())
}

/// Prompts the user for a string input with a default value.
///
/// Displays a prompt message with a suggested default value. If the user
/// just presses Enter, the default value is returned.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
/// * `default` - Optional default value to suggest
///
/// # Returns
///
/// * `Result<String>` - The user's input or default value
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

/// Prompts the user for a yes/no confirmation.
///
/// Accepts 'y', 'yes', 'n', 'no' (case insensitive).
/// Empty input is treated as 'no'.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
///
/// # Returns
///
/// * `Result<bool>` - true if user confirms, false otherwise
///
/// # Errors
///
/// Returns an error if reading from stdin fails.
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

/// Prompts the user for a positive floating-point number.
///
/// Displays a prompt message and waits for the user to enter a number.
/// Invalid or non-positive numbers will show an error and re-prompt.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
///
/// # Returns
///
/// * `Result<f64>` - The user's input as a positive float
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

/// Prompts the user for a floating-point number with a default value.
///
/// Shows a default value and allows the user to press Enter to accept it,
/// or enter a new positive number. Invalid inputs will show an error and re-prompt.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
/// * `default` - Optional default value to suggest
///
/// # Returns
///
/// * `Result<f64>` - The user's input or default value as a positive float
pub fn prompt_float_with_default(prompt: &str, default: Option<f64>) -> Result<f64> {
    loop {
        let input: String = if let Some(default_val) = default {
            prompt_string(&format!("{prompt} [{default_val:.1}]"))?
        } else {
            prompt_string(prompt)?
        };

        if input.trim().is_empty()
            && let Some(default_val) = default
        {
            return Ok(default_val);
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
