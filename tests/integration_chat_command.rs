#![cfg(unix)]

//! Integration tests for the chat command launching llama-cli.

use anyhow::Result;
use chrono::Utc;
use gglib::commands::chat::{ChatCommandArgs, handle_chat};
use gglib::models::Gguf;
use gglib::services::database;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;

fn write_stub_llama_cli(bin_path: &Path) -> Result<()> {
    let script = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "llama-cli stub"
  exit 0
fi

if [ -n "$LLAMA_CLI_LOG" ]; then
  printf "%s\n" "$@" >> "$LLAMA_CLI_LOG"
fi

exit 0
"#;

    fs::write(bin_path, script)?;
    let mut perms = fs::metadata(bin_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(bin_path, perms)?;
    Ok(())
}

async fn seed_database(model_path: &Path) -> Result<Gguf> {
    let pool = database::setup_database().await?;

    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), "Chat Test".to_string());

    let model = Gguf {
        id: None,
        name: "Chat Test".to_string(),
        file_path: model_path.to_path_buf(),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    database::add_model(&pool, &model).await?;
    let stored = database::list_models(&pool)
        .await?
        .into_iter()
        .next()
        .expect("model should exist");
    Ok(stored)
}

#[tokio::test]
async fn chat_command_passes_expected_arguments() {
    let repo_dir = tempdir().unwrap();
    let repo_path = repo_dir.path();
    let old_repo = std::env::var("GGLIB_RESOURCE_DIR").ok();
    let old_data = std::env::var("GGLIB_DATA_DIR").ok();
    unsafe {
        std::env::set_var("GGLIB_RESOURCE_DIR", repo_path);
        std::env::set_var("GGLIB_DATA_DIR", repo_path);
    }

    let ll_bin_dir = repo_path.join(".llama/bin");
    fs::create_dir_all(&ll_bin_dir).unwrap();
    let llama_cli_path = ll_bin_dir.join("llama-cli");
    write_stub_llama_cli(&llama_cli_path).unwrap();

    // Also create a stub for llama-server to satisfy ensure_llama_initialized()
    let llama_server_path = ll_bin_dir.join("llama-server");
    write_stub_llama_cli(&llama_server_path).unwrap();

    let model_path = repo_path.join("chat.gguf");
    fs::write(&model_path, b"test").unwrap();
    let model_path = fs::canonicalize(&model_path).unwrap_or(model_path);

    let template_file = repo_path.join("tmpl.jinja");
    fs::write(&template_file, "{{ user }}").unwrap();

    let model = seed_database(&model_path).await.unwrap();

    let log_path = repo_path.join("llama-cli.log");
    let prev_log = std::env::var("LLAMA_CLI_LOG").ok();
    unsafe {
        std::env::set_var("LLAMA_CLI_LOG", &log_path);
    }

    handle_chat(ChatCommandArgs {
        identifier: model.name.clone(),
        ctx_size: Some("max".into()),
        mlock: true,
        chat_template: Some("llama3".into()),
        chat_template_file: Some(template_file.to_string_lossy().into()),
        jinja: true,
        system_prompt: Some("Be helpful".into()),
        multiline_input: true,
        simple_io: true,
    })
    .await
    .unwrap();

    let log_contents = fs::read_to_string(&log_path).unwrap();
    let tokens: Vec<&str> = log_contents.split_whitespace().collect();
    assert!(tokens.contains(&"-m"));
    assert!(tokens.contains(&model_path.to_str().unwrap()));
    assert!(tokens.contains(&"-c"));
    assert!(tokens.contains(&"4096"));
    assert!(tokens.contains(&"--mlock"));
    assert!(tokens.contains(&"--interactive-first"));
    assert!(tokens.contains(&"--multiline-input"));
    assert!(tokens.contains(&"--simple-io"));
    assert!(tokens.contains(&"-sys"));
    assert!(tokens.contains(&"Be"));
    assert!(tokens.contains(&"helpful"));
    assert!(tokens.contains(&"--chat-template"));
    assert!(tokens.contains(&"llama3"));

    if let Some(prev) = old_repo {
        unsafe {
            std::env::set_var("GGLIB_RESOURCE_DIR", prev);
        }
    } else {
        unsafe {
            std::env::remove_var("GGLIB_RESOURCE_DIR");
        }
    }

    if let Some(prev) = old_data {
        unsafe {
            std::env::set_var("GGLIB_DATA_DIR", prev);
        }
    } else {
        unsafe {
            std::env::remove_var("GGLIB_DATA_DIR");
        }
    }

    if let Some(prev) = prev_log {
        unsafe {
            std::env::set_var("LLAMA_CLI_LOG", prev);
        }
    } else {
        unsafe {
            std::env::remove_var("LLAMA_CLI_LOG");
        }
    }
}
