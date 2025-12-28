/**
 * Shared server lifecycle event normalization.
 *
 * Keeps Tauri and Web(SSE) behavior identical by translating backend event payloads
 * into the `serverRegistry` event union.
 */

import type { ServerEvent } from './serverRegistry';

export type CanonicalServerEventName =
  | 'server:snapshot'
  | 'server:started'
  | 'server:stopped'
  | 'server:error'
  | 'server:health_changed';

function toRecord(payload: unknown): Record<string, unknown> | null {
  if (typeof payload !== 'object' || payload === null) return null;
  return payload as Record<string, unknown>;
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

        const modelId = String(entry.modelId ?? entry.model_id ?? '');
        if (!modelId) return null;

        const port = typeof entry.port === 'number' ? entry.port : undefined;

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
        return { modelId, status: 'running' as const, port, updatedAt };
      })
      .filter((x): x is NonNullable<typeof x> => x !== null),
  };
}

function normalizeHealthChanged(data: Record<string, unknown>): ServerEvent | null {
  const modelId = String(data.modelId ?? data.model_id ?? '');
  if (!modelId) return null;

  const status = data.status as Record<string, unknown> | undefined;
  if (!status || typeof status.status !== 'string') return null;

  const detail = typeof data.detail === 'string' ? data.detail : undefined;

  const updatedAt =
    typeof data.timestamp === 'number'
      ? (coerceUnixTimeToMs(data.timestamp) ?? data.timestamp)
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

function normalizeLifecycle(
  kind: 'running' | 'stopped' | 'crashed',
  data: Record<string, unknown>
): ServerEvent | null {
  const modelId = String(data.modelId ?? data.model_id ?? '');
  if (!modelId) return kind === 'crashed' ? null : null;

  const port = typeof data.port === 'number' ? data.port : undefined;

  const updatedAt =
    typeof data.updatedAt === 'number'
      ? data.updatedAt
      : typeof data.updated_at === 'number'
        ? data.updated_at
        : Date.now();

  if (kind === 'running') return { type: 'running', modelId, port, updatedAt };
  if (kind === 'stopped') return { type: 'stopped', modelId, port, updatedAt };

  // server:error may omit modelId on the Rust side; ignore in that case.
  return modelId ? { type: 'crashed', modelId, port, updatedAt } : null;
}

/**
 * Normalize a named canonical server:* event (Tauri uses event names).
 */
export function normalizeServerEventFromNamedEvent(
  eventName: CanonicalServerEventName,
  payload: unknown
): ServerEvent | null {
  const data = toRecord(payload);
  if (!data) return null;

  switch (eventName) {
    case 'server:snapshot':
      return normalizeSnapshot(data);
    case 'server:started':
      return normalizeLifecycle('running', data);
    case 'server:stopped':
      return normalizeLifecycle('stopped', data);
    case 'server:error':
      return normalizeLifecycle('crashed', data);
    case 'server:health_changed':
      return normalizeHealthChanged(data);
    default:
      return null;
  }
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
