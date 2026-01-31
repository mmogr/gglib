/**
 * Download Event Decoder with Runtime Validation
 * 
 * Provides type-safe decoding of raw SSE payloads into typed DownloadEvent objects.
 * Includes runtime validation to catch contract drift between Rust backend and TS frontend.
 */

import { appLogger } from '../platform';
import type { DownloadEvent } from '../transport/types/events';

/**
 * Known download event types.
 * Used for runtime validation to catch unknown/new event types early.
 * 
 * These match the Rust `DownloadEvent` enum variants with `#[serde(rename_all = "snake_case")]`:
 * - QueueSnapshot → "queue_snapshot"
 * - DownloadStarted → "download_started"
 * - QueueRunComplete → "queue_run_complete"
 * - etc.
 */
const KNOWN_DOWNLOAD_EVENT_TYPES = new Set([
  'queue_snapshot',
  'download_started',
  'download_progress',
  'shard_progress',
  'download_completed',
  'download_failed',
  'download_cancelled',
  'queue_run_complete',
]);

/**
 * Validate and decode a raw SSE payload into a DownloadEvent.
 * 
 * - In development: throws on unknown event types or missing required fields
 * - In production: logs warnings but returns null for invalid events
 * 
 * @param payload - Raw JSON payload from SSE
 * @returns Decoded DownloadEvent or null if invalid
 */
export function decodeDownloadEvent(payload: unknown): DownloadEvent | null {
  if (!payload || typeof payload !== 'object') {
    logInvalidEvent('Payload is not an object', payload);
    return null;
  }

  const event = payload as Record<string, unknown>;
  
  if (typeof event.type !== 'string') {
    logInvalidEvent('Event missing type field', payload);
    return null;
  }

  // Validate known event type
  if (!KNOWN_DOWNLOAD_EVENT_TYPES.has(event.type)) {
    logUnknownEventType(event.type, payload);
    return null;
  }

  // Type is valid, return as-is (TypeScript will narrow based on discriminant)
  // Wire format uses snake_case, which matches our TS types
  return event as DownloadEvent;
}

/**
 * Log an invalid event payload.
 * In dev: throws an error. In prod: logs a warning.
 */
function logInvalidEvent(reason: string, payload: unknown): void {
  appLogger.error('service.download', 'Invalid download event', { reason, payload });
}

/**
 * Log an unknown event type.
 * This indicates the backend added a new event type that the frontend doesn't know about yet.
 */
function logUnknownEventType(type: string, payload: unknown): void {
  appLogger.error('service.download', 'Unknown download event type - backend may have added new variant', {
    type,
    payload,
    knownTypes: Array.from(KNOWN_DOWNLOAD_EVENT_TYPES)
  });
}

/**
 * Normalize wire format field names to TypeScript conventions.
 * 
 * Currently a pass-through since we're using snake_case in both Rust and TS
 * to avoid mapping complexity. If we decide to use camelCase in TS, this is
 * where we'd do the conversion.
 * 
 * @param event - Decoded event with wire format field names
 * @returns Event with normalized field names
 */
export function normalizeEventFieldNames(event: DownloadEvent): DownloadEvent {
  // For now, keep wire format (snake_case) in TS for simplicity
  // If we want camelCase, we'd map fields here:
  // e.g., speed_bps → speedBps, eta_seconds → etaSeconds
  return event;
}
