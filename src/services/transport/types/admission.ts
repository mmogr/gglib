/**
 * Admission-control transport types.
 *
 * Aliases over the bindings generated from
 * `crates/gglib-core/src/domain/admission.rs`. It was a hand-written mirror
 * until the generator covered these types.
 *
 * Kept apart from `dashboard.ts` rather than folded into it: these describe
 * the runtime's VRAM residency and request queue, which is a different
 * subject from llama.cpp's inference slots and prompt cache, and the snapshot
 * re-exports them so a component reading one need not know the difference.
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
