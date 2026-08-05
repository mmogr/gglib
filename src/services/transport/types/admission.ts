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
export type SecondarySlotState =
  | 'resident'
  | 'available'
  | 'too_large'
  | 'no_headroom'
  | 'unknown_footprint'
  | 'unknown_budget';

/** Mirrors `gglib_core::domain::SecondarySlotStatus`. */
export interface SecondarySlotStatus {
  /** Machine-readable label, for styling. Branch on this, not on `detail`. */
  state: SecondarySlotState;
  /** Ready-to-render explanation. */
  detail: string;
}

/** Mirrors `gglib_core::domain::ResidentSlotSnapshot` — one model in VRAM. */
export interface ResidentSlotSnapshot {
  /** Slot index; `0` is the primary. */
  slot: number;
  model_name: string;
  model_id: number;
  port: number;
  /**
   * Requests currently holding a lease on this slot. A slot serving anything
   * at all is never evicted — a swap must not preempt a live generation.
   */
  inflight: number;
  /** Whether this is the slot chat traffic and the `/slots` poller follow. */
  is_primary: boolean;
  resident_for_secs: number;
}

/** Mirrors `gglib_core::domain::QueuedModelSnapshot` — requests waiting for one model. */
export interface QueuedModelSnapshot {
  model_name: string;
  waiting: number;
  /** Age of the oldest waiter, in milliseconds. */
  oldest_wait_ms: number;
}

/**
 * Mirrors `gglib_core::domain::AdmissionSnapshot` — who holds VRAM, who is
 * queued behind them, and why the second slot is or is not in use.
 *
 * A non-empty `queued` means traffic is being batched behind a model swap,
 * which is the queue working rather than a fault. Comparing `total_queued`
 * against `total_swaps` shows how much that batching saved.
 */
export interface AdmissionSnapshot {
  slots: ResidentSlotSnapshot[];
  queued: QueuedModelSnapshot[];
  total_queued: number;
  total_swaps: number;
  secondary_slot: SecondarySlotStatus;
}
