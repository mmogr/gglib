//! Whether llama-server's disk slot save/restore can actually resume a model.
//!
//! gglib's disk slot layer persists a conversation's KV state to
//! `--slot-save-path` and restores it via `/slots?action=save|restore` when
//! that conversation comes back. That round-trip only resumes the prompt when
//! the model's KV memory retains the full token history.
//!
//! ## Why partial-memory models can't use it
//!
//! llama-server's slot save serializes the sequence KV state and token list
//! but *not* the server's context checkpoints, and its restore path clears the
//! slot's checkpoint list. For a full-attention model that's sufficient — the
//! KV state alone resumes the prompt. For sliding-window, hybrid, and
//! recurrent architectures it isn't: resuming needs history those layers no
//! longer hold, which llama-server bridges with context checkpoints. Finding
//! none after a restore, it falls back to `n_past = 0` and re-prefills the
//! *entire* prompt.
//!
//! Worse, the restore is not merely useless there — it is actively harmful.
//! Pre-filling the slot makes llama-server's own similarity check pick the
//! restored slot and skip consulting its in-RAM prompt cache, which *does*
//! carry checkpoints and would have resumed cheaply. So on these models the
//! disk layer converts a fast RAM-cache hit into a full re-prefill, on top of
//! writing gigabytes per generation for nothing.
//!
//! gglib therefore disables the disk layer for partial-memory models and lets
//! the host-RAM prompt cache (`--cache-ram`, auto-sized per launch) handle
//! conversation switching on its own.
//!
//! TODO: revisit once llama.cpp's slot files carry `prompt.checkpoints`
//! through save/restore (`SERVER_TASK_TYPE_SLOT_SAVE` / `_RESTORE` in
//! `tools/server/server-context.cpp`). When they do, this resolution can
//! return `Supported` unconditionally. `GGLIB_FORCE_HYBRID_DISK_CACHE=1`
//! re-enables the layer for testing that upstream fix without a rebuild.

use crate::system::is_truthy_flag;

/// Indicates how the disk slot-restore decision was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRestoreSource {
    /// Full-attention model — the disk layer works normally.
    Supported,
    /// Sliding-window, hybrid, or recurrent KV memory — layer disabled.
    UnsupportedPartialKv,
    /// Partial KV memory, but `GGLIB_FORCE_HYBRID_DISK_CACHE` re-enabled the
    /// layer anyway.
    ForcedByEnv,
}

/// Resolved disk slot-layer decision for a llama-server launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotRestoreResolution {
    /// Whether the disk slot save/restore layer should be used at all.
    pub enabled: bool,
    /// Why [`Self::enabled`] came out the way it did.
    pub source: SlotRestoreSource,
}

impl SlotRestoreResolution {
    /// A one-line, human-readable explanation, or `None` for the ordinary
    /// supported case (nothing unusual to explain).
    #[must_use]
    pub fn explain(&self) -> Option<String> {
        match self.source {
            SlotRestoreSource::Supported => None,
            SlotRestoreSource::UnsupportedPartialKv => Some(
                "disk KV slot cache disabled: this model's attention keeps only part of the \
                 token history (sliding-window/hybrid/recurrent), and llama-server's slot \
                 files omit the context checkpoints needed to resume it — restoring one \
                 would force a full prompt re-prefill. Relying on the host-RAM prompt cache \
                 instead; override with GGLIB_FORCE_HYBRID_DISK_CACHE=1"
                    .to_string(),
            ),
            SlotRestoreSource::ForcedByEnv => Some(
                "disk KV slot cache force-enabled via GGLIB_FORCE_HYBRID_DISK_CACHE on a \
                 partial-KV-memory model — restores are expected to re-prefill the full prompt"
                    .to_string(),
            ),
        }
    }
}

/// Whether `GGLIB_FORCE_HYBRID_DISK_CACHE` asks for the disk slot layer to
/// stay on even for partial-KV-memory models.
///
/// Counterpart to the `GGLIB_DISABLE_<FEATURE>` convention used by
/// `GGLIB_DISABLE_KV_QUANT` and `GGLIB_DISABLE_CACHE_AUTOSIZE`; phrased as
/// `FORCE` because it re-enables a feature gglib turns off on its own.
fn hybrid_disk_cache_forced_via_env() -> bool {
    std::env::var("GGLIB_FORCE_HYBRID_DISK_CACHE")
        .ok()
        .is_some_and(|v| is_truthy_flag(&v))
}

/// Resolve whether the disk slot layer should be used for a launch.
///
/// # Arguments
///
/// * `kv_memory_is_partial` — from
///   [`gglib_core::domain::kv_memory_is_partial`], via
///   `ModelLaunchSpec::kv_memory_is_partial`.
#[must_use]
pub fn resolve_slot_restore(kv_memory_is_partial: bool) -> SlotRestoreResolution {
    resolve_slot_restore_with(kv_memory_is_partial, hybrid_disk_cache_forced_via_env())
}

/// [`resolve_slot_restore`] with the environment override supplied explicitly.
///
/// Split out so the decision itself is testable without mutating process-wide
/// environment state (this crate denies `unsafe`, which `set_var` requires).
#[must_use]
fn resolve_slot_restore_with(
    kv_memory_is_partial: bool,
    forced_by_env: bool,
) -> SlotRestoreResolution {
    if !kv_memory_is_partial {
        return SlotRestoreResolution {
            enabled: true,
            source: SlotRestoreSource::Supported,
        };
    }

    if forced_by_env {
        return SlotRestoreResolution {
            enabled: true,
            source: SlotRestoreSource::ForcedByEnv,
        };
    }

    SlotRestoreResolution {
        enabled: false,
        source: SlotRestoreSource::UnsupportedPartialKv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_attention_model_keeps_the_disk_layer() {
        let got = resolve_slot_restore_with(false, false);
        assert!(got.enabled);
        assert_eq!(got.source, SlotRestoreSource::Supported);
        assert_eq!(got.explain(), None, "supported case needs no explanation");
    }

    #[test]
    fn partial_kv_model_disables_the_disk_layer() {
        let got = resolve_slot_restore_with(true, false);
        assert!(!got.enabled);
        assert_eq!(got.source, SlotRestoreSource::UnsupportedPartialKv);
        let msg = got.explain().expect("disabled case explains itself");
        assert!(msg.contains("GGLIB_FORCE_HYBRID_DISK_CACHE"), "{msg}");
    }

    #[test]
    fn env_override_re_enables_for_partial_kv_models() {
        let got = resolve_slot_restore_with(true, true);
        assert!(got.enabled);
        assert_eq!(got.source, SlotRestoreSource::ForcedByEnv);
        assert!(got.explain().is_some());
    }

    /// The override is scoped to the partial case — it must not relabel a
    /// model that never needed rescuing.
    #[test]
    fn env_override_does_not_relabel_supported_models() {
        let got = resolve_slot_restore_with(false, true);
        assert!(got.enabled);
        assert_eq!(got.source, SlotRestoreSource::Supported);
    }

    /// The public entry point reads the environment; with the variable unset
    /// in the normal test environment it must agree with the pure form.
    #[test]
    fn public_resolver_matches_pure_form_without_the_override() {
        if std::env::var("GGLIB_FORCE_HYBRID_DISK_CACHE").is_ok() {
            return; // caller set the override; nothing to assert
        }
        assert_eq!(
            resolve_slot_restore(true),
            resolve_slot_restore_with(true, false)
        );
        assert_eq!(
            resolve_slot_restore(false),
            resolve_slot_restore_with(false, false)
        );
    }

    #[test]
    fn truthy_flag_parsing_drives_the_override() {
        // Guards the `is_truthy_flag` contract this module depends on.
        assert!(is_truthy_flag("1"));
        assert!(!is_truthy_flag("0"));
        assert!(!is_truthy_flag(""));
    }
}
