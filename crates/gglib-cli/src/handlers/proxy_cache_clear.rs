//! `gglib proxy cache-clear` — clear KV cache via the proxy HTTP endpoint.
//!
//! Connects to `POST /v1/proxy/cache/clear` on a running `gglib proxy` (or
//! `gglib web`) instance and clears slot files for all sessions or a specific
//! session ID.

use anyhow::{Context, Result};

/// Clear KV cache via the proxy's `/v1/proxy/cache/clear` endpoint.
pub async fn execute(host: &str, port: u16, session_id: Option<&str>) -> Result<()> {
    let url = format!("http://{}:{}/v1/proxy/cache/clear", host, port);

    let mut builder = reqwest::Client::new().post(&url);

    if let Some(sid) = session_id {
        builder = builder.header("X-Gglib-Session-Id", sid);
    }

    let response = builder.send().await.context(format!(
        "Failed to connect to proxy at {} — is it running?",
        url
    ))?;

    let status = response.status();
    let body = response.text().await.ok();

    match status.as_u16() {
        200 => {
            println!("Cache cleared: {}", body.unwrap_or_default());
            Ok(())
        }
        400 => {
            eprintln!("Bad request: {}", body.unwrap_or_default());
            std::process::exit(1);
        }
        _ => {
            eprintln!(
                "Proxy returned status {}: {}",
                status,
                body.unwrap_or_default()
            );
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_unreachable_proxy_returns_error() {
        // Use a port that nothing is listening on — simulates proxy not running
        let result = execute("127.0.0.1", 59999, None).await;
        assert!(result.is_err(), "Expected error when proxy is unreachable");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to connect") || err_msg.contains("Connection refused"),
            "Error message should mention connection failure, got: {}",
            err_msg
        );
    }
}
