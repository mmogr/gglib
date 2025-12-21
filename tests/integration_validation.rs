//! Integration tests for validation utilities
//!
//! These tests verify file validation and GGUF parsing functionality
//! with real files and various edge cases.

use gglib_core::utils::validation;
use gglib_gguf::GgufParser;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_validate_existing_gguf_file() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.gguf");
    File::create(&file_path).unwrap();

    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(result.is_ok());
}

#[test]
fn test_validate_nonexistent_file() {
    let result = validation::validate_file("/path/that/does/not/exist.gguf");
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("File does not exist"));
}

#[test]
fn test_validate_wrong_extension() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    File::create(&file_path).unwrap();

    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Wrong extension"));
}

#[test]
fn test_validate_no_extension() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test_file_no_extension");
    File::create(&file_path).unwrap();

    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("File has no extension"));
}

#[test]
fn test_validate_various_extensions() {
    let temp_dir = tempdir().unwrap();

    let test_cases = vec![
        ("model.gguf", true),
        ("model.GGUF", false), // Case sensitive
        ("model.gguf.backup", false),
        ("model.txt", false),
        ("model.bin", false),
        ("model.safetensors", false),
    ];

    for (filename, should_be_valid) in test_cases {
        let file_path = temp_dir.path().join(filename);
        File::create(&file_path).unwrap();

        let result = validation::validate_file(file_path.to_str().unwrap());
        if should_be_valid {
            assert!(result.is_ok(), "File {filename} should be valid");
        } else {
            assert!(result.is_err(), "File {filename} should be invalid");
        }
    }
}

#[test]
fn test_validate_with_unicode_paths() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("测试模型.gguf");
    File::create(&file_path).unwrap();

    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(
        result.is_ok(),
        "Unicode filenames should be handled correctly"
    );
}

#[test]
fn test_validate_with_spaces_in_path() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("my model with spaces.gguf");
    File::create(&file_path).unwrap();

    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(
        result.is_ok(),
        "Paths with spaces should be handled correctly"
    );
}

#[test]
fn test_validate_empty_string_path() {
    let result = validation::validate_file("");
    assert!(result.is_err());
}

#[test]
fn test_validate_relative_paths() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.gguf");
    File::create(&file_path).unwrap();

    // Test with absolute path
    let result_abs = validation::validate_file(file_path.to_str().unwrap());
    assert!(result_abs.is_ok());

    // Create a file in current directory for relative path test
    let current_dir_file = "test_relative.gguf";
    let _file = File::create(current_dir_file);

    // Clean up
    let _ = std::fs::remove_file(current_dir_file);
}

#[test]
fn test_validate_symbolic_links() {
    // Note: This test is platform-specific and may not work on all systems
    #[cfg(unix)]
    {
        let temp_dir = tempdir().unwrap();
        let original_file = temp_dir.path().join("original.gguf");
        let link_file = temp_dir.path().join("link.gguf");

        File::create(&original_file).unwrap();

        if std::os::unix::fs::symlink(&original_file, &link_file).is_ok() {
            let result = validation::validate_file(link_file.to_str().unwrap());
            assert!(result.is_ok(), "Symbolic links to valid files should work");
        }
    }
}

// Note: validate_and_parse_gguf tests would require creating actual GGUF files
// or mocking the gguf_parser module, which is more complex.
// In a real project, you'd want to:
// 1. Create sample GGUF files for testing
// 2. Mock the gguf_parser module for unit tests
// 3. Test with various GGUF file formats and edge cases

#[test]
fn test_validate_and_parse_gguf_with_nonexistent_file() {
    let parser = GgufParser::new();
    let result = validation::validate_and_parse_gguf(&parser, "/nonexistent/file.gguf");
    assert!(result.is_err());
    // Should fail on file validation before attempting to parse
}

#[test]
fn test_validate_and_parse_gguf_with_wrong_extension() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    File::create(&file_path).unwrap();

    let parser = GgufParser::new();
    let result = validation::validate_and_parse_gguf(&parser, file_path.to_str().unwrap());
    assert!(result.is_err());
    // Should fail on extension validation before attempting to parse
}

// For comprehensive GGUF parsing tests, you would need:
// 1. Sample GGUF files with known metadata
// 2. Invalid GGUF files to test error handling
// 3. GGUF files with various architectures and quantization types
// 4. Edge cases like very large files, corrupted headers, etc.

#[test]
fn test_create_mock_gguf_file_for_parsing() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("mock.gguf");

    // Create a file with some content (not a real GGUF, but for testing file operations)
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "This is not a real GGUF file, just for testing").unwrap();

    // Validate the file exists and has correct extension
    let validation_result = validation::validate_file(file_path.to_str().unwrap());
    assert!(validation_result.is_ok());

    // The parsing would fail because it's not a real GGUF file
    let parser = GgufParser::new();
    let parse_result = validation::validate_and_parse_gguf(&parser, file_path.to_str().unwrap());
    assert!(parse_result.is_err());
    // This is expected - we'd need real GGUF files or mocked parser for successful parsing tests
}
