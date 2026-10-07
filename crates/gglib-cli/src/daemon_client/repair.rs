//! A model's repair, on a [`DaemonHandle`].
//!
//! The daemon deletes the model's unhealthy files and queues the download
//! that fetches them again on its own queue, where `gglib model download`
//! queues, so the download is run and can be followed there.

use std::time::Duration;

use anyhow::Result;
use gglib_core::services::RepairStarted;

use super::wire::RepairBody;
use super::{DaemonHandle, paths};

/// How long the daemon may take to answer a repair. With no shard named it
/// hashes every file of the model before it deletes any, which for a large
/// model on a slow disk is minutes.
const REPAIR_TIMEOUT: Duration = Duration::from_mins(30);

impl DaemonHandle {
    /// Have the daemon repair the model `model_id`: the shards named, or
    /// every unhealthy file when none is. Answers the download it queued,
    /// and the files that download is to bring back.
    pub(crate) async fn repair_model(
        &self,
        model_id: i64,
        shards: Option<Vec<usize>>,
    ) -> Result<RepairStarted> {
        let response = self
            .post(&paths::model_repair_path(model_id))
            .json(&RepairBody { shards })
            .timeout(REPAIR_TIMEOUT)
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
