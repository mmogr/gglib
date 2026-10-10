//! A stored [`Model`] made without a database, for tests and repository
//! doubles.

use super::{Model, NewModel};

impl Model {
    /// The row a repository answers after storing `new` under `id`: every
    /// field `new` carries, and nothing a launch or a benchmark adds later.
    #[must_use]
    pub fn stored(id: i64, new: &NewModel) -> Self {
        Self {
            id,
            name: new.name.clone(),
            model_key: String::new(),
            file_path: new.file_path.clone(),
            projector_path: new.projector_path.clone(),
            param_count_b: new.param_count_b,
            architecture: new.architecture.clone(),
            quantization: new.quantization.clone(),
            context_length: new.context_length,
            expert_count: new.expert_count,
            expert_used_count: new.expert_used_count,
            expert_shared_count: new.expert_shared_count,
            metadata: new.metadata.clone(),
            added_at: new.added_at,
            hf_repo_id: new.hf_repo_id.clone(),
            hf_commit_sha: new.hf_commit_sha.clone(),
            hf_filename: new.hf_filename.clone(),
            download_date: new.download_date,
            last_update_check: new.last_update_check,
            tags: new.tags.clone(),
            capabilities: new.capabilities,
            inference_defaults: new.inference_defaults.clone(),
            defaults_origin: new.defaults_origin,
            server_defaults: new.server_defaults.clone(),
            dialect_spec: new.dialect_spec.clone(),
            template_caps: None,
            benchmark_summary: None,
            image_family: new.image_family,
            components: new.components.clone(),
        }
    }
}
