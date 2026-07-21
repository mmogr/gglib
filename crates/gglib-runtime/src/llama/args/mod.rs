#![doc = include_str!("README.md")]
pub mod cache_ram;
pub mod jinja;
pub mod kv_cache_type;
pub mod mtp;
pub mod reasoning;
pub mod slot_restore;

// Re-export public API
pub use cache_ram::{CacheRamResolution, CacheRamSource, resolve_cache_ram};
pub use jinja::{JinjaResolution, JinjaResolutionSource, resolve_jinja_flag};
pub use kv_cache_type::{KvCacheTypeResolution, KvCacheTypeSource, resolve_kv_cache_types};
pub use mtp::{
    DEFAULT_DRAFT_N_MAX, DEFAULT_DRAFT_P_MIN, MtpResolution, MtpResolutionSource, resolve_mtp_args,
};
pub use reasoning::{
    ReasoningDetection, ReasoningFormatResolution, ReasoningFormatSource, resolve_reasoning_format,
    resolve_reasoning_format_with_detection,
};
pub use slot_restore::{SlotRestoreResolution, SlotRestoreSource, resolve_slot_restore};
