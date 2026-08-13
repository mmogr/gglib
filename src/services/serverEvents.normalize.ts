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

import type { ServerEvent } from './serverRegistry';
import type { RuntimeErrorInfo } from '../types';

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

function normalizeSnapshot(data: Record<string, unknown>): ServerEvent | null {
  const servers = data.servers;
  if (!Array.isArray(servers)) return null;

  return {
    type: 'snapshot',
    servers: servers
      .map((s) => {
        if (typeof s !== 'object' || s === null) return null;
        const entry = s as Record<string, unknown>;

        const modelId = coerceModelId(entry.modelId ?? entry.model_id);
        if (!modelId) return null;

        const port = typeof entry.port === 'number' ? entry.port : undefined;
        const modelName = typeof entry.modelName === 'string' ? entry.modelName
          : typeof entry.model_name === 'string' ? entry.model_name
          : undefined;

        const startedAtRaw =
          typeof entry.startedAt === 'number'
            ? entry.startedAt
            : typeof entry.started_at === 'number'
              ? entry.started_at
              : undefined;

        const updatedAt =
          coerceUnixTimeToMs(startedAtRaw) ??
          (typeof entry.updatedAt === 'number'
            ? entry.updatedAt
            : typeof entry.updated_at === 'number'
              ? entry.updated_at
              : Date.now());

        // Snapshot only lists running servers.
        return { modelId, status: 'running' as const, port, updatedAt, modelName };
      })
      .filter((x): x is NonNullable<typeof x> => x !== null),
  };
}

function normalizeHealthChanged(data: Record<string, unknown>): ServerEvent | null {
  const modelId = coerceModelId(data.modelId ?? data.model_id);
  if (!modelId) return null;

  const status = data.status as Record<string, unknown> | undefined;
  if (!status || typeof status.status !== 'string') return null;

  const detail = typeof data.detail === 'string' ? data.detail : undefined;

  const updatedAt =
    typeof data.timestamp === 'number'
      ? (coerceUnixTimeToMs(data.timestamp) ?? Date.now())
      : typeof data.updatedAt === 'number'
        ? data.updatedAt
        : typeof data.updated_at === 'number'
          ? data.updated_at
          : Date.now();

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
  const modelId = coerceModelId(data.modelId ?? data.model_id);
  if (!modelId) return null;

  const port = typeof data.port === 'number' ? data.port : undefined;
  const modelName = typeof data.modelName === 'string' ? data.modelName
    : typeof data.model_name === 'string' ? data.model_name
    : undefined;

  const updatedAt =
    typeof data.updatedAt === 'number'
      ? data.updatedAt
      : typeof data.updated_at === 'number'
        ? data.updated_at
        : Date.now();

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
