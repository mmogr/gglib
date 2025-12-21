//! Server service - orchestrates server lifecycle operations.

use crate::ports::{CoreError, ProcessHandle, ProcessRunner, ServerConfig, ServerHealth};
use std::sync::Arc;

/// Service for managing model server processes.
pub struct ServerService {
    runner: Arc<dyn ProcessRunner>,
}

impl ServerService {
    /// Create a new server service.
    pub fn new(runner: Arc<dyn ProcessRunner>) -> Self {
        Self { runner }
    }

    /// Start a model server.
    pub async fn start(&self, config: ServerConfig) -> Result<ProcessHandle, CoreError> {
        self.runner.start(config).await.map_err(CoreError::from)
    }

    /// Stop a model server.
    pub async fn stop(&self, handle: &ProcessHandle) -> Result<(), CoreError> {
        self.runner.stop(handle).await.map_err(CoreError::from)
    }

    /// Check if a server is still running.
    pub async fn is_running(&self, handle: &ProcessHandle) -> bool {
        self.runner.is_running(handle).await
    }

    /// Get health status of a server.
    pub async fn health(&self, handle: &ProcessHandle) -> Result<ServerHealth, CoreError> {
        self.runner.health(handle).await.map_err(CoreError::from)
    }

    /// List all running server handles.
    pub async fn list_running(&self) -> Result<Vec<ProcessHandle>, CoreError> {
        self.runner.list_running().await.map_err(CoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ProcessError, ProcessRunner};
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockRunner {
        handles: Mutex<Vec<ProcessHandle>>,
        next_port: Mutex<u16>,
    }

    impl MockRunner {
        fn new(base_port: u16) -> Self {
            Self {
                handles: Mutex::new(vec![]),
                next_port: Mutex::new(base_port),
            }
        }
    }

    #[async_trait]
    impl ProcessRunner for MockRunner {
        async fn start(&self, config: ServerConfig) -> Result<ProcessHandle, ProcessError> {
            let port = {
                let mut next = self.next_port.lock().unwrap();
                let port = *next;
                *next += 1;
                port
            };
            let handle = ProcessHandle::new(
                config.model_id,
                config.model_name,
                Some(12345),
                port,
                0, // started_at
            );
            self.handles.lock().unwrap().push(handle.clone());
            Ok(handle)
        }

        async fn stop(&self, handle: &ProcessHandle) -> Result<(), ProcessError> {
            self.handles
                .lock()
                .unwrap()
                .retain(|h| h.port != handle.port);
            Ok(())
        }

        async fn is_running(&self, handle: &ProcessHandle) -> bool {
            self.handles
                .lock()
                .unwrap()
                .iter()
                .any(|h| h.port == handle.port)
        }

        async fn health(&self, handle: &ProcessHandle) -> Result<ServerHealth, ProcessError> {
            let handles = self.handles.lock().unwrap();
            if handles.iter().any(|h| h.port == handle.port) {
                Ok(ServerHealth::healthy())
            } else {
                Err(ProcessError::NotRunning(format!("port={}", handle.port)))
            }
        }

        async fn list_running(&self) -> Result<Vec<ProcessHandle>, ProcessError> {
            Ok(self.handles.lock().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn test_start_and_stop() {
        let runner = Arc::new(MockRunner::new(9000));
        let service = ServerService::new(runner);

        let config = ServerConfig::new(
            1,
            "test-model".to_string(),
            PathBuf::from("/path/to/model.gguf"),
            9000,
        );

        let handle = service.start(config).await.unwrap();
        assert_eq!(handle.port, 9000);

        let running = service.list_running().await.unwrap();
        assert_eq!(running.len(), 1);

        service.stop(&handle).await.unwrap();
        let running = service.list_running().await.unwrap();
        assert!(running.is_empty());
    }
}
