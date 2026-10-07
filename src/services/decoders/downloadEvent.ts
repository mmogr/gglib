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
 * These are the five variants of the Rust `DownloadEvent` enum, tagged with
 * `#[serde(rename_all = "snake_case")]`. The `Record` is keyed by the
 * generated union's tags, so a variant added or removed in Rust fails to
 * compile here until this list follows.
 */
const DOWNLOAD_EVENT_TYPES: Record<DownloadEvent['type'], true> = {
  queue_snapshot: true,
  download_completed: true,
  download_failed: true,
  download_cancelled: true,
  queue_run_complete: true,
};
const KNOWN_DOWNLOAD_EVENT_TYPES = new Set<string>(Object.keys(DOWNLOAD_EVENT_TYPES));

/**
 * Validate and decode a raw SSE payload into a DownloadEvent.
 * 
 * Only the `type` tag is checked: a payload that is not an object, has no
 * string `type`, or has a `type` that is not one of the known five is logged
 * as an error and decodes to null, in development and production alike.
 * Nothing throws, and the fields beside the tag are not checked.
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
 * Log an invalid event payload as an error.
 */
function logInvalidEvent(reason: string, payload: unknown): void {
  appLogger.error('service.download', 'Invalid download event', { reason, payload });
}

/**
 * Log an unknown event type as an error.
 * This indicates the backend added a new event type that the frontend doesn't know about yet.
 */
function logUnknownEventType(type: string, payload: unknown): void {
  appLogger.error('service.download', 'Unknown download event type - backend may have added new variant', {
    type,
    payload,
    knownTypes: Array.from(KNOWN_DOWNLOAD_EVENT_TYPES)
  });
}

