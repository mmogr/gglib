/**
 * Admission-control transport types.
 *
 * Mirrors `gglib_core::domain::admission` (Rust,
 * `crates/gglib-core/src/domain/admission.rs`) bit-for-bit — field names and
 * casing match the wire format exactly.
 *
 * Split out of `dashboard.ts` rather than sitting alongside the rest of the
 * snapshot: these describe the runtime's VRAM residency and request queue,
 * which is a different subject from llama.cpp's inference slots and prompt
 * cache, and keeping them apart stops either file growing past the point where
 * it can be read in one sitting.
 *
 * The same local-mirror caveat applies as in `dashboard.ts`: this is a real
 * HTTP client of the JSON contract, not a shared type, and it tolerates
 * additive server-side changes by ignoring unknown fields.
 */

/**
 * Why the second VRAM slot is or is not in use. Mirrors
 * `gglib_core::domain::SecondarySlotStatus.state`.
 *
 * `resident` — a second model is co-loaded and never waits for a swap.
 * `available` — nothing is co-loaded, and nothing has been refused.
 * The rest are refusals: too large for the slot, not enough free VRAM, a
 * footprint that could not be estimated, or a machine whose free VRAM gglib
 * cannot read (everything but NVIDIA and Apple Silicon).
 */
import type { SecondarySlotStatus } from '../../../types/generated/SecondarySlotStatus';

export type SecondarySlotState = SecondarySlotStatus['state'];

export type { SecondarySlotStatus };

/** One model in VRAM. */
export type { ResidentSlotSnapshot } from '../../../types/generated/ResidentSlotSnapshot';

/** Requests waiting for one model. */
export type { QueuedModelSnapshot } from '../../../types/generated/QueuedModelSnapshot';

/**
 * Who holds VRAM, who is queued behind them, and why the second slot is or is
 * not in use.
 *
 * A non-empty `queued` means traffic is being batched behind a model swap,
 * which is the queue working rather than a fault. Comparing `total_queued`
 * against `total_swaps` shows how much that batching saved.
 */
export type { AdmissionSnapshot } from '../../../types/generated/AdmissionSnapshot';
