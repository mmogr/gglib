//! Assistant-UI runtime handlers.
//!
//! Thin wrappers for assistant-ui npm package management.

use std::path::PathBuf;
use std::process::Command;

/// Handle assistant-ui install command
pub fn handle_install() -> Result<(), String> {
    println!("Installing assistant-ui dependencies...");

    // Check if npm is available
    let npm_check = Command::new("npm").arg("--version").output();

    match npm_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("Found npm version: {}", version.trim());
        }
        _ => {
            return Err("npm not found. Please install Node.js and npm first.".to_string());
        }
    }

    // Run npm install
    println!("Running npm install...");
    let install_result = Command::new("npm").arg("install").status();

    match install_result {
        Ok(status) if status.success() => {
            println!("✓ assistant-ui dependencies installed successfully");

            // Verify installation
            if verify_installation() {
                println!("✓ Installation verified");
                Ok(())
            } else {
                Err("Installation verification failed".to_string())
            }
        }
        Ok(status) => Err(format!(
            "npm install failed with exit code: {:?}",
            status.code()
        )),
        Err(e) => Err(format!("Failed to run npm install: {}", e)),
    }
}

/// Handle assistant-ui update command
pub fn handle_update() -> Result<(), String> {
    println!("Updating assistant-ui dependencies...");

    let update_result = Command::new("npm")
        .arg("update")
        .arg("@assistant-ui/react")
        .arg("react-markdown")
        .arg("remark-gfm")
        .arg("rehype-highlight")
        .status();

    match update_result {
        Ok(status) if status.success() => {
            println!("✓ assistant-ui dependencies updated successfully");
            Ok(())
        }
        Ok(status) => Err(format!(
            "npm update failed with exit code: {:?}",
            status.code()
        )),
        Err(e) => Err(format!("Failed to run npm update: {}", e)),
    }
}

/// Handle assistant-ui status command
pub fn handle_status() -> Result<(), String> {
    println!("Checking assistant-ui installation status...\n");

    // Check npm
    let npm_check = Command::new("npm").arg("--version").output();

    match npm_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("✓ npm: {}", version.trim());
        }
        _ => {
            println!("✗ npm: not found");
            return Err("npm not available".to_string());
        }
    }

    // Check if node_modules exists
    let node_modules = PathBuf::from("node_modules");
    if !node_modules.exists() {
        println!("✗ node_modules not found");
        println!("\nRun 'gglib assistant-ui install' to install dependencies");
        return Err("Dependencies not installed".to_string());
    }

    println!("✓ node_modules directory exists");

    // Check each required package
    let packages = vec![
        "@assistant-ui/react",
        "react-markdown",
        "remark-gfm",
        "rehype-highlight",
    ];

    let mut all_installed = true;
    for package in packages {
        let package_path = if package.starts_with('@') {
            let parts: Vec<&str> = package.split('/').collect();
            PathBuf::from("node_modules").join(parts[0]).join(parts[1])
        } else {
            PathBuf::from("node_modules").join(package)
        };

        if package_path.exists() {
            println!("✓ {}", package);
        } else {
            println!("✗ {} (not found)", package);
            all_installed = false;
        }
    }

    if all_installed {
        println!("\n✓ All assistant-ui dependencies are installed");
        Ok(())
    } else {
        println!("\n✗ Some dependencies are missing");
        println!("Run 'gglib assistant-ui install' to install missing dependencies");
        Err("Missing dependencies".to_string())
    }
}

/// Verify assistant-ui installation
fn verify_installation() -> bool {
    let critical_packages = vec!["@assistant-ui/react", "@assistant-ui/react-ai-sdk"];

    for package in critical_packages {
        let parts: Vec<&str> = package.split('/').collect();
        let package_path = PathBuf::from("node_modules").join(parts[0]).join(parts[1]);

        if !package_path.exists() {
            println!("✗ Critical package {} not found", package);
            return false;
        }
    }

    true
}
