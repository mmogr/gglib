//! Model verification service for integrity checking and update detection.
//!
//! This service provides:
//! - Integrity verification via SHA256 hash comparison against HuggingFace OIDs
//! - Update detection by comparing local OIDs with remote repository state
//! - Model repair by re-downloading corrupt or missing shards
//! - Concurrency control to prevent conflicting operations on the same model

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use crate::domain::ModelFile;
use crate::ports::{
    HfClientPort, ModelRepository, RepositoryError,
};

// ============================================================================
// Domain Types
// ============================================================================

/// Progress status for an individual shard verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShardProgress {
    /// Verification starting for this shard.
    Starting,
    /// Currently hashing the file.
    Hashing {
        /// Percentage complete (0-100).
        percent: u8,
        /// Bytes processed so far.
        bytes_processed: u64,
        /// Total bytes in the file.
        total_bytes: u64,
    },
    /// Verification completed for this shard.
    Completed {
        /// Health status of this shard.
        health: ShardHealth,
    },
}

/// Health status of an individual shard after verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShardHealth {
    /// File is healthy - hash matches expected OID.
    Healthy,
    /// File is corrupt - hash doesn't match expected OID.
    Corrupt {
        /// Expected SHA256 hash (from HuggingFace OID).
        expected: String,
        /// Actual computed SHA256 hash.
        actual: String,
    },
    /// File is missing from disk.
    Missing,
    /// No OID available to verify against.
    NoOid,
}

/// Progress update during model verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationProgress {
    /// Model ID being verified.
    pub model_id: i64,
    /// Current shard index being verified.
    pub shard_index: usize,
    /// Total number of shards.
    pub total_shards: usize,
    /// Progress status for this shard.
    pub shard_progress: ShardProgress,
}

/// Complete verification report for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    /// Model ID that was verified.
    pub model_id: i64,
    /// Overall health status.
    pub overall_health: OverallHealth,
    /// Health status for each shard.
    pub shards: Vec<ShardHealthReport>,
    /// When the verification was performed.
    pub verified_at: chrono::DateTime<Utc>,
}

/// Overall health status for a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverallHealth {
    /// All shards are healthy.
    Healthy,
    /// One or more shards are corrupt or missing.
    Unhealthy,
    /// No OIDs available for verification.
    Unverifiable,
}

/// Health report for a single shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardHealthReport {
    /// Shard index.
    pub index: usize,
    /// File path.
    pub file_path: String,
    /// Health status.
    pub health: ShardHealth,
}

/// Result of checking for model updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    /// Model ID that was checked.
    pub model_id: i64,
    /// Whether an update is available.
    pub update_available: bool,
    /// Details about what changed (if update available).
    pub details: Option<UpdateDetails>,
}

/// Details about available updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDetails {
    /// Number of shards that have changed.
    pub changed_shards: usize,
    /// OID changes per shard.
    pub changes: Vec<ShardUpdate>,
}

/// Update information for a single shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardUpdate {
    /// Shard index.
    pub index: usize,
    /// File path.
    pub file_path: String,
    /// Old OID (local).
    pub old_oid: String,
    /// New OID (remote).
    pub new_oid: String,
}

/// Type of operation being performed on a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// Model is being verified.
    Verifying,
    /// Model is being downloaded/repaired.
    Downloading,
}

// ============================================================================
// Concurrency Control
// ============================================================================

/// RAII guard that automatically releases the operation lock when dropped.
pub struct OperationGuard {
    model_id: i64,
    lock_map: Arc<RwLock<HashMap<i64, OperationType>>>,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let model_id = self.model_id;
        let lock_map: Arc<RwLock<HashMap<i64, OperationType>>> = Arc::clone(&self.lock_map);
        
        // Spawn a task to release the lock asynchronously
        tokio::spawn(async move {
            let mut map = lock_map.write().await;
            map.remove(&model_id);
        });
    }
}

/// Concurrency control for model operations.
///
/// Ensures only one operation of each type can run on a model at a time.
pub struct ModelOperationLock {
    locks: Arc<RwLock<HashMap<i64, OperationType>>>,
}

impl ModelOperationLock {
    /// Create a new operation lock manager.
    pub fn new() -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Try to acquire a lock for the specified operation.
    ///
    /// Returns `Ok(guard)` if the lock was acquired, or `Err` if another
    /// operation is already in progress for this model.
    pub async fn try_acquire(
        &self,
        model_id: i64,
        operation: OperationType,
    ) -> Result<OperationGuard, String> {
        let mut map = self.locks.write().await;
        
        if let Some(existing) = map.get(&model_id) {
            return Err(format!(
                "Model {} is already locked for {:?} operation",
                model_id, existing
            ));
        }
        
        map.insert(model_id, operation);
        
        Ok(OperationGuard {
            model_id,
            lock_map: Arc::clone(&self.locks),
        })
    }
}

impl Default for ModelOperationLock {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Service
// ============================================================================

/// Port trait for accessing model files repository.
///
/// This is a minimal trait that wraps the concrete ModelFilesRepository
/// to avoid circular dependencies.
#[async_trait]
pub trait ModelFilesReaderPort: Send + Sync {
    /// Get all model files for a specific model.
    async fn get_by_model_id(&self, model_id: i64) -> anyhow::Result<Vec<ModelFile>>;
    
    /// Update the last verified timestamp for a model file.
    async fn update_verification_time(
        &self,
        id: i64,
        verified_at: chrono::DateTime<Utc>,
    ) -> anyhow::Result<()>;
}

/// Port trait for triggering downloads.
///
/// This abstracts the download manager to avoid tight coupling.
#[async_trait]
pub trait DownloadTriggerPort: Send + Sync {
    /// Queue a download for a specific model by repo ID and quantization.
    async fn queue_download(
        &self,
        repo_id: String,
        quantization: Option<String>,
    ) -> anyhow::Result<String>;
}

/// Model verification service.
pub struct ModelVerificationService {
    /// Repository for model metadata.
    model_repo: Arc<dyn ModelRepository>,
    /// Repository for model file metadata.
    model_files_repo: Arc<dyn ModelFilesReaderPort>,
    /// HuggingFace client for update checks.
    hf_client: Arc<dyn HfClientPort>,
    /// Download trigger for repairs.
    download_trigger: Arc<dyn DownloadTriggerPort>,
    /// Concurrency control.
    operation_lock: ModelOperationLock,
}

impl ModelVerificationService {
    /// Create a new verification service.
    pub fn new(
        model_repo: Arc<dyn ModelRepository>,
        model_files_repo: Arc<dyn ModelFilesReaderPort>,
        hf_client: Arc<dyn HfClientPort>,
        download_trigger: Arc<dyn DownloadTriggerPort>,
    ) -> Self {
        Self {
            model_repo,
            model_files_repo,
            hf_client,
            download_trigger,
            operation_lock: ModelOperationLock::new(),
        }
    }

    /// Verify the integrity of a model by computing SHA256 hashes.
    ///
    /// Returns a channel for progress updates and a handle to the verification task.
    ///
    /// # Arguments
    ///
    /// * `model_id` - ID of the model to verify
    ///
    /// # Returns
    ///
    /// * `receiver` - Channel for receiving progress updates
    /// * `handle` - Join handle for the verification task
    pub async fn verify_model_integrity(
        &self,
        model_id: i64,
    ) -> Result<(mpsc::Receiver<VerificationProgress>, JoinHandle<Result<VerificationReport, RepositoryError>>), String> {
        // Acquire lock
        let _guard = self.operation_lock.try_acquire(model_id, OperationType::Verifying).await?;

        // Get model and file metadata
        let _model = self.model_repo.get_by_id(model_id).await
            .map_err(|e| format!("Failed to get model: {}", e))?;
        
        let model_files = self.model_files_repo.get_by_model_id(model_id).await
            .map_err(|e| format!("Failed to get model files: {}", e))?;

        if model_files.is_empty() {
            return Err("No model files found for verification".to_string());
        }

        let total_shards = model_files.len();

        // Create progress channel
        let (tx, rx) = mpsc::channel(100);

        // Clone dependencies for the async task
        let model_files_repo = Arc::clone(&self.model_files_repo);
        let _model_repo = Arc::clone(&self.model_repo);

        // Spawn verification task
        let handle = tokio::spawn(async move {
            let mut shard_reports = Vec::new();
            let mut has_unhealthy = false;
            let mut all_unverifiable = true;

            for (index, file) in model_files.iter().enumerate() {
                // Send starting progress
                let _ = tx.send(VerificationProgress {
                    model_id,
                    shard_index: index,
                    total_shards,
                    shard_progress: ShardProgress::Starting,
                }).await;

                let health = Self::verify_shard(
                    file,
                    model_id,
                    index,
                    total_shards,
                    &tx,
                ).await;

                // Update verification timestamp
                if let Err(e) = model_files_repo.update_verification_time(file.id, Utc::now()).await {
                    tracing::warn!(
                        model_id = model_id,
                        file_id = file.id,
                        error = %e,
                        "Failed to update verification timestamp"
                    );
                }

                // Track overall health
                match &health {
                    ShardHealth::Corrupt { .. } | ShardHealth::Missing => has_unhealthy = true,
                    ShardHealth::Healthy => all_unverifiable = false,
                    ShardHealth::NoOid => {},
                }

                shard_reports.push(ShardHealthReport {
                    index,
                    file_path: file.file_path.clone(),
                    health: health.clone(),
                });

                // Send completion progress
                let _ = tx.send(VerificationProgress {
                    model_id,
                    shard_index: index,
                    total_shards,
                    shard_progress: ShardProgress::Completed { health },
                }).await;
            }

            let overall_health = if has_unhealthy {
                OverallHealth::Unhealthy
            } else if all_unverifiable {
                OverallHealth::Unverifiable
            } else {
                OverallHealth::Healthy
            };

            Ok(VerificationReport {
                model_id,
                overall_health,
                shards: shard_reports,
                verified_at: Utc::now(),
            })
        });

        Ok((rx, handle))
    }

    /// Verify a single shard by computing its SHA256 and comparing with OID.
    async fn verify_shard(
        file: &ModelFile,
        model_id: i64,
        index: usize,
        total_shards: usize,
        tx: &mpsc::Sender<VerificationProgress>,
    ) -> ShardHealth {
        // Check if OID is available
        let Some(ref expected_oid) = file.hf_oid else {
            return ShardHealth::NoOid;
        };

        let file_path = Path::new(&file.file_path);

        // Check if file exists
        if !file_path.exists() {
            return ShardHealth::Missing;
        }

        // Compute SHA256 in a blocking task
        let path_owned = file_path.to_path_buf();
        let tx_clone = tx.clone();

        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let mut file = File::open(&path_owned)?;
            let total_bytes = file.metadata()?.len();
            
            let mut hasher = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024]; // 1MB chunks
            let mut bytes_processed = 0u64;
            
            // Initial progress
            let _ = tx_clone.blocking_send(VerificationProgress {
                model_id,
                shard_index: index,
                total_shards,
                shard_progress: ShardProgress::Hashing {
                    percent: 0,
                    bytes_processed: 0,
                    total_bytes,
                },
            });
            
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                
                hasher.update(&buffer[..n]);
                bytes_processed += n as u64;
                
                // Report progress every ~100MB or at end
                if bytes_processed % (100 * 1024 * 1024) < (1024 * 1024)
                    || bytes_processed == total_bytes
                {
                    #[allow(clippy::cast_possible_truncation)]
                    let percent = ((bytes_processed as f64 / total_bytes as f64) * 100.0) as u8;
                    
                    let _ = tx_clone.blocking_send(VerificationProgress {
                        model_id,
                        shard_index: index,
                        total_shards,
                        shard_progress: ShardProgress::Hashing {
                            percent,
                            bytes_processed,
                            total_bytes,
                        },
                    });
                }
            }
            
            Ok(format!("{:x}", hasher.finalize()))
        }).await;

        match result {
            Ok(Ok(computed_hash)) => {
                if computed_hash == *expected_oid {
                    ShardHealth::Healthy
                } else {
                    ShardHealth::Corrupt {
                        expected: expected_oid.clone(),
                        actual: computed_hash,
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::error!(
                    model_id = model_id,
                    file_path = %file.file_path,
                    error = %e,
                    "Failed to compute hash"
                );
                ShardHealth::Missing
            }
            Err(e) => {
                tracing::error!(
                    model_id = model_id,
                    file_path = %file.file_path,
                    error = %e,
                    "Task panicked during hash computation"
                );
                ShardHealth::Missing
            }
        }
    }

    /// Check if updates are available for a model.
    ///
    /// Compares local OIDs with remote OIDs from HuggingFace.
    pub async fn check_for_updates(&self, model_id: i64) -> Result<UpdateCheckResult, RepositoryError> {
        // Get model metadata
        let model = self.model_repo.get_by_id(model_id).await?;
        
        let Some(ref repo_id) = model.hf_repo_id else {
            return Ok(UpdateCheckResult {
                model_id,
                update_available: false,
                details: None,
            });
        };

        let Some(ref quantization) = model.quantization else {
            return Ok(UpdateCheckResult {
                model_id,
                update_available: false,
                details: None,
            });
        };

        // Get local file metadata
        let local_files = self.model_files_repo.get_by_model_id(model_id).await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        if local_files.is_empty() {
            return Ok(UpdateCheckResult {
                model_id,
                update_available: false,
                details: None,
            });
        }

        // Get remote file metadata from HuggingFace
        let remote_files = self.hf_client
            .get_quantization_files(repo_id, quantization)
            .await
            .map_err(|e| RepositoryError::Storage(format!("Failed to fetch remote files: {}", e)))?;

        // Compare OIDs
        let mut changes = Vec::new();

        for local_file in &local_files {
            let Some(ref local_oid) = local_file.hf_oid else {
                continue;
            };

            // Find matching remote file by path
            if let Some(remote_file) = remote_files.iter().find(|f| f.path == local_file.file_path) {
                if let Some(ref remote_oid) = remote_file.oid {
                    if local_oid != remote_oid {
                        let old_oid_str: String = local_oid.clone();
                        let new_oid_str: String = remote_oid.clone();
                        changes.push(ShardUpdate {
                            index: local_file.file_index as usize,
                            file_path: local_file.file_path.clone(),
                            old_oid: old_oid_str,
                            new_oid: new_oid_str,
                        });
                    }
                }
            }
        }

        let update_available = !changes.is_empty();
        let details = if update_available {
            Some(UpdateDetails {
                changed_shards: changes.len(),
                changes,
            })
        } else {
            None
        };

        Ok(UpdateCheckResult {
            model_id,
            update_available,
            details,
        })
    }

    /// Repair a model by re-downloading corrupt or missing shards.
    ///
    /// # Arguments
    ///
    /// * `model_id` - ID of the model to repair
    /// * `shard_indices` - Optional list of specific shard indices to repair.
    ///                     If `None`, all unhealthy shards will be repaired.
    pub async fn repair_model(
        &self,
        model_id: i64,
        shard_indices: Option<Vec<usize>>,
    ) -> Result<String, String> {
        // Acquire downloading lock
        let _guard = self.operation_lock.try_acquire(model_id, OperationType::Downloading).await?;

        // Get model metadata
        let model = self.model_repo.get_by_id(model_id).await
            .map_err(|e| format!("Failed to get model: {}", e))?;

        let Some(ref repo_id) = model.hf_repo_id else {
            return Err("Model does not have HuggingFace repository information".to_string());
        };

        let Some(ref quantization) = model.quantization else {
            return Err("Model does not have quantization information".to_string());
        };

        // Get file metadata
        let model_files = self.model_files_repo.get_by_model_id(model_id).await
            .map_err(|e| format!("Failed to get model files: {}", e))?;

        // Determine which shards to repair
        let shards_to_repair: Vec<&ModelFile> = if let Some(indices) = shard_indices {
            model_files.iter()
                .filter(|f| indices.contains(&(f.file_index as usize)))
                .collect()
        } else {
            // Verify all shards to find unhealthy ones
            let mut unhealthy = Vec::new();
            for file in &model_files {
                let (tx, _rx) = mpsc::channel(1);
                let health = Self::verify_shard(file, model_id, 0, 1, &tx).await;
                match health {
                    ShardHealth::Corrupt { .. } | ShardHealth::Missing => {
                        unhealthy.push(file);
                    }
                    _ => {}
                }
            }
            unhealthy
        };

        if shards_to_repair.is_empty() {
            return Err("No unhealthy shards found to repair".to_string());
        }

        // Delete corrupt/missing files
        for file in &shards_to_repair {
            let path = Path::new(&file.file_path);
            if path.exists() {
                if let Err(e) = tokio::fs::remove_file(path).await {
                    tracing::warn!(
                        model_id = model_id,
                        file_path = %file.file_path,
                        error = %e,
                        "Failed to delete corrupt file"
                    );
                }
            }
        }

        // Trigger re-download
        let download_id = self.download_trigger
            .queue_download(repo_id.clone(), Some(quantization.clone()))
            .await
            .map_err(|e| format!("Failed to queue download: {}", e))?;

        Ok(download_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_operation_lock_single_acquire() {
        let lock = ModelOperationLock::new();
        let guard = lock.try_acquire(1, OperationType::Verifying).await;
        assert!(guard.is_ok());
    }

    #[tokio::test]
    async fn test_operation_lock_double_acquire_fails() {
        let lock = ModelOperationLock::new();
        let _guard1 = lock.try_acquire(1, OperationType::Verifying).await.unwrap();
        let guard2 = lock.try_acquire(1, OperationType::Downloading).await;
        assert!(guard2.is_err());
    }

    #[tokio::test]
    async fn test_operation_lock_release_on_drop() {
        let lock = ModelOperationLock::new();
        {
            let _guard = lock.try_acquire(1, OperationType::Verifying).await.unwrap();
        }
        // Give the drop task time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let guard2 = lock.try_acquire(1, OperationType::Downloading).await;
        assert!(guard2.is_ok());
    }

    #[tokio::test]
    async fn test_operation_lock_different_models() {
        let lock = ModelOperationLock::new();
        let guard1 = lock.try_acquire(1, OperationType::Verifying).await;
        let guard2 = lock.try_acquire(2, OperationType::Verifying).await;
        assert!(guard1.is_ok());
        assert!(guard2.is_ok());
    }
}
