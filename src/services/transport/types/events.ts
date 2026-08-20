/**
 * Events transport types.
 * Handles real-time event subscriptions.
 */


// ============================================================================
// Server Events
// ============================================================================

/**
 * The server frames `/api/events` actually carries.
 *
 * These are `AppEvent`'s server variants as serde writes them — snake_case
 * tags, camelCase fields, `modelId` a *number*. Deliberately not the shape
 * consumers work with: `serverEvents.normalize` turns these into the
 * registry's `ServerEvent`, which is keyed by string and speaks in
 * running/stopping/stopped/crashed. Two shapes, because there really are two.
 */
import type { AppEvent } from '../../../types/generated/AppEvent';

/**
 * The `server_*` slice of `AppEvent`.
 *
 * An `Extract` rather than a re-declaration, so the arms cannot drift. Note
 * what that buys and what it does not: a new `server_*` arm added in Rust
 * joins this type with no TypeScript error anywhere, and
 * `normalizeServerEventFromAppEvent` will drop it in its `default:`. The
 * compiler cannot flag that, because widening a union it consumes is not a
 * type error. A new arm needs a normalizer case written by hand.
 */
export type ServerWireEvent = Extract<AppEvent, { type: `server_${string}` }>;

// ============================================================================
// Download Events
// ============================================================================

/**
 * One row of the download queue.
 *
 * `status` is the full seven-member `DownloadStatus`, where the mirror named
 * five. A snapshot row only ever carries `downloading` or `queued` —
 * `gglib-download` hard-codes both — so the two the mirror omitted never
 * arrived, and nothing was misrendered. The type is simply the type.
 *
 * `error`, `group_id` and `shard_info` are optional and *not* nullable: each
 * carries `skip_serializing_if`, so the key is absent rather than `null`. The
 * mirror admitted both, which is why the normalizer reached for them through
 * `as any` casts.
 */
import type { DownloadSummary } from '../../../types/generated/DownloadSummary';
export type { DownloadSummary };

/**
 * Result kind for a completion attempt.
 */
export type { CompletionKind } from '../../../types/generated/CompletionKind';

/**
 * Details for a single completed artifact in a queue run.
 *
 * `download_ids` carries the structured wire id — `{ model_id, quantization }`
 * — not the `"model_id:quantization"` string that `./ids` calls a
 * `DownloadId`. Two different things share that name, so the generated one is
 * imported under its own alias rather than shadowing the string form, which
 * the rest of the transport still uses.
 */
import type { CompletionDetail } from '../../../types/generated/CompletionDetail';
export type { CompletionDetail };

/**
 * Summary of an entire queue run from start to drain.
 * Emitted when the queue transitions from busy → idle.
 *
 * The last hand-written type in this file, and the only drift class it could
 * still hide was field *addition*: `useDownloadManager` assigns the generated
 * `DownloadEvent.summary` into state typed by this, so a removal or a type
 * change already failed to compile, while a new Rust field simply stayed
 * unreadable. Nothing gated it either — `check_ts_bindings.sh` and
 * `make bindings-check` only ever look at Rust and `src/types/generated`.
 */
import type { QueueRunSummary } from '../../../types/generated/QueueRunSummary';
export type { QueueRunSummary };

export type { DownloadEvent } from '../../../types/generated/DownloadEvent';

// ============================================================================
// Model Events
// ============================================================================

/**
 * The lightweight model shape library events carry — deliberately not the full
 * model row, since a listener's job is to notice the change, not to render
 * from the notification.
 *
 * Mirrors `gglib_core::events::ModelSummary` (camelCase).
 */
export type { ModelSummary as ModelEventSummary } from '../../../types/generated/ModelSummary';

/**
 * Library changes, broadcast to every client attached to the daemon.
 *
 * These let a second window or browser tab reach a list that would otherwise
 * refresh only when its own tab made the edit. A `gglib model add` in a
 * terminal is a separate process and does not reach here.
 */
export type ModelEvent = Extract<AppEvent, { type: `model_${string}` }>;

// ============================================================================
// Verification Events
// ============================================================================

export type { OverallHealth } from '../../../types/generated/OverallHealth';

export type VerificationEvent = Extract<AppEvent, { type: `verification_${string}` }>;

// ============================================================================
// Proxy Events
// ============================================================================

export type ProxyEvent = Extract<AppEvent, { type: `proxy_${string}` }>;

// ============================================================================
// App Event Map
// ============================================================================

/**
 * Each subscribable category, and the slice of `AppEvent` it delivers.
 *
 * Every value is a named slice and never a bare `AppEvent`. That is the point
 * of the map: a handler registered for `'proxy'` should not have to narrow
 * against `server_started`, and typing any entry as the whole union would let
 * a `server` handler compile while reading a `download` payload.
 *
 * `getEventCategory` is what actually routes a message here, and it is
 * ordinary runtime code — so these five slices are a claim about the router,
 * not a guarantee from it. The five together are exhaustive over `AppEvent`'s
 * fourteen arms today, which `tests/ts/services/eventCategory.test.ts` is the
 * place to keep true.
 *
 * Download events arrive wrapped as `{ type: "download", event: DownloadEvent }`
 * to preserve shard-level detail, which is why that entry is the whole arm
 * rather than the inner event.
 */
export interface AppEventMap {
  'server': ServerWireEvent;
  'download': Extract<AppEvent, { type: 'download' }>;
  'model': ModelEvent;
  'verification': VerificationEvent;
  'proxy': ProxyEvent;
}

export type AppEventType = keyof AppEventMap;

// ============================================================================
// Events Transport Interface
// ============================================================================
