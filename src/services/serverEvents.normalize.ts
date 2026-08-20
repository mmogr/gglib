/**
 * Server lifecycle event normalization.
 *
 * Translates the daemon's `AppEvent` payloads into the `serverRegistry` event
 * union, tolerantly: unknown types, malformed records and ids that will not
 * coerce are dropped rather than becoming garbage registry keys.
 *
 * There used to be a second entry point here keyed by Tauri event *name*
 * (`server:started` and friends), because the desktop app listened on the
 * Tauri bus while the web build listened over SSE. Nothing emitted those
 * names once the GUI backend moved into the daemon, so both surfaces now
 * arrive through `normalizeServerEventFromAppEvent`.
 */

import type { ServerEvent, ServerStateInfo } from './serverRegistry';
import type { RuntimeErrorInfo, ServerInfo } from '../types';

function toRecord(payload: unknown): Record<string, unknown> | null {
  if (typeof payload !== 'object' || payload === null) return null;
  return payload as Record<string, unknown>;
}

/**
 * Coerce a payload model id into a registry key.
 * Backend ids are numeric; anything that doesn't coerce to a finite number
 * (missing field, "undefined", "NaN") would otherwise become a garbage
 * registry key that the UI renders literally.
 */
function coerceModelId(value: unknown): string | null {
  const id = String(value ?? '');
  if (!id || !Number.isFinite(Number(id))) return null;
  return id;
}

function coerceUnixTimeToMs(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) return null;

  // Heuristics:
  // - seconds: ~1e9 .. 1e10
  // - milliseconds: ~1e12 .. 1e13
  // - nanoseconds: ~1e18
  if (value >= 1e17) return Math.floor(value / 1e6); // ns -> ms
  if (value >= 1e11) return Math.floor(value); // already ms
  return Math.floor(value * 1000); // seconds -> ms
}

/**
 * One `snapshot` entry, from the fields both producers supply under their own
 * spellings. Shared so the two entry points below cannot drift on how a
 * server becomes a registry row — only on how they read one off the wire.
 */
function snapshotEntry(fields: {
  modelId: unknown;
  modelName?: string;
  port?: number;
  startedAt?: number;
}): ServerStateInfo | null {
  const modelId = coerceModelId(fields.modelId);
  if (!modelId) return null;

  return {
    modelId,
    // Both producers list only running servers.
    status: 'running',
    port: fields.port,
    updatedAt: coerceUnixTimeToMs(fields.startedAt) ?? Date.now(),
    modelName: fields.modelName,
  };
}

const present = <T,>(x: T | null): x is T => x !== null;

function normalizeSnapshot(data: Record<string, unknown>): ServerEvent | null {
  const servers = data.servers;
  if (!Array.isArray(servers)) return null;

  return {
    type: 'snapshot',
    servers: servers
      .map((s) => {
        if (typeof s !== 'object' || s === null) return null;
        const entry = s as Record<string, unknown>;

        return snapshotEntry({
          modelId: entry.modelId,
          modelName: typeof entry.modelName === 'string' ? entry.modelName : undefined,
          port: typeof entry.port === 'number' ? entry.port : undefined,
          startedAt: typeof entry.startedAt === 'number' ? entry.startedAt : undefined,
        });
      })
      .filter(present),
  };
}

/**
 * Hydration from `GET /api/servers`.
 *
 * A separate entry point rather than a snake_case fallback inside
 * [`normalizeServerEventFromAppEvent`], because this is not an `AppEvent` and
 * never was — it is a REST list the caller re-wrapped to look like one. The
 * two producers really do disagree: `ServerSnapshotEntry` is camelCase, while
 * `ServerInfo` is snake_case and carries a `pid` the registry has no use for.
 * Naming both paths is what lets each read exactly the shape its own producer
 * sends, instead of one tolerant reader accepting either and documenting
 * neither.
 *
 * Total, not `null`-returning: there is no whole-payload failure to report,
 * only individual entries — malformed, or with an id that will not coerce —
 * and those are dropped as they are on the event path.
 */
export function normalizeServerSnapshotFromList(
  servers: ServerInfo[],
): Extract<ServerEvent, { type: 'snapshot' }> {
  return {
    type: 'snapshot',
    servers: servers
      .map((s) =>
        // `ServerInfo[]` is what the endpoint promises, not what it
        // guarantees — the fetch behind it is an unchecked cast. A null entry
        // would throw on property access, and the caller swallows rejections,
        // so one bad row would silently cost the whole hydration. The event
        // path drops the row and keeps the rest; so does this.
        toRecord(s) === null
          ? null
          : snapshotEntry({
              modelId: s.model_id,
              modelName: s.model_name,
              port: s.port,
              startedAt: s.started_at,
            }),
      )
      .filter(present),
  };
}

function normalizeHealthChanged(data: Record<string, unknown>): ServerEvent | null {
  const modelId = coerceModelId(data.modelId);
  if (!modelId) return null;

  const status = data.status as Record<string, unknown> | undefined;
  if (!status || typeof status.status !== 'string') return null;

  const detail = typeof data.detail === 'string' ? data.detail : undefined;

  // `timestamp` is a non-optional `u64` on the Rust variant, so it is always
  // there; `Date.now()` covers only a malformed frame.
  const updatedAt = coerceUnixTimeToMs(data.timestamp) ?? Date.now();

  return {
    type: 'server_health_changed',
    modelId,
    status: status as import('../types').ServerHealthStatus,
    detail,
    updatedAt,
  };
}

function toRuntimeErrorInfo(value: unknown): RuntimeErrorInfo | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const err = value as Record<string, unknown>;

  if (
    typeof err.message !== 'string' ||
    typeof err.type !== 'string' ||
    typeof err.retryable !== 'boolean'
  ) {
    return undefined;
  }

  return { message: err.message, type: err.type, retryable: err.retryable };
}

function normalizeLifecycle(
  kind: 'running' | 'stopped' | 'crashed',
  data: Record<string, unknown>
): ServerEvent | null {
  const modelId = coerceModelId(data.modelId);
  if (!modelId) return null;

  const port = typeof data.port === 'number' ? data.port : undefined;
  const modelName = typeof data.modelName === 'string' ? data.modelName : undefined;

  // No server lifecycle variant carries a timestamp of its own — arrival time
  // is the only clock there is for these.
  const updatedAt = Date.now();

  if (kind === 'running') return { type: 'running', modelId, port, updatedAt, modelName };
  if (kind === 'stopped') return { type: 'stopped', modelId, port, updatedAt, modelName };

  // server:error may omit modelId on the Rust side; ignore in that case.
  return { type: 'crashed', modelId, port, updatedAt, modelName, error: toRuntimeErrorInfo(data.error) };
}

/**
 * Normalize a backend AppEvent payload coming from SSE.
 *
 * SSE payloads are AppEvent objects tagged with snake_case `type`, e.g.:
 * - { type: 'server_started', modelId: 1, port: 8080 }
 */
export function normalizeServerEventFromAppEvent(payload: unknown): ServerEvent | null {
  const data = toRecord(payload);
  if (!data) return null;

  const t = data.type;
  if (typeof t !== 'string') return null;

  switch (t) {
    case 'server_snapshot':
      return normalizeSnapshot(data);
    case 'server_started':
      return normalizeLifecycle('running', data);
    case 'server_stopped':
      return normalizeLifecycle('stopped', data);
    case 'server_error':
      return normalizeLifecycle('crashed', data);
    case 'server_health_changed':
      return normalizeHealthChanged(data);
    default:
      return null;
  }
}
