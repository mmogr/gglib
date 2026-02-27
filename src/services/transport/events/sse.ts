/**
 * SSE (Server-Sent Events) handling for transport layer.
 *
 * Uses fetch-based streaming with Bearer token authentication.
 * All event types share a **single** SSE connection to `/api/events` to avoid
 * exhausting the browser's HTTP/1.1 per-origin connection limit (6 slots).
 * Events are demultiplexed client-side and dispatched to type-specific handlers.
 *
 * @see https://github.com/mmogr/gglib/issues/301
 */

import type { Unsubscribe, EventHandler } from '../types/common';
import type { AppEventType, AppEventMap } from '../types/events';
import { decodeDownloadEvent } from '../../decoders/downloadEvent';
import { appLogger } from '../../platform';
import { createSSEStream, type SSEMessage } from '../../../utils/sse';
import { getApiBaseUrl, getAuthHeaders, getClient } from '../api/client';

/**
 * Unified SSE endpoint path.
 *
 * All events (server, download, log, voice, verification) are multiplexed
 * through a single SSE connection at this endpoint.
 */
export const SSE_EVENTS_ENDPOINT = '/api/events';

/**
 * Parse JSON safely, returning null on error.
 */
function safeJsonParse(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return null;
  }
}

/**
 * Parse app event from SSE message.
 * Backend sends JSON in data: field with {type: "...", ...} structure.
 */
function parseAppEvent(msg: SSEMessage): unknown {
  const data = safeJsonParse(msg.data);
  // Backend uses default event type, payload is in data
  return data;
}

/**
 * Exponential backoff with jitter for reconnection.
 */
class Backoff {
  private ms = 500;
  private readonly maxMs: number;

  constructor(minMs = 500, maxMs = 30000) {
    this.ms = minMs;
    this.maxMs = maxMs;
  }

  next(): number {
    const jitter = Math.floor(Math.random() * 250);
    const out = Math.min(this.ms, this.maxMs) + jitter;
    this.ms = Math.min(this.ms * 2, this.maxMs);
    return out;
  }

  reset(): void {
    this.ms = 500;
  }
}

/**
 * SSE connection manager with automatic reconnection.
 * Supports multiple subscribers sharing a single connection.
 */
export class SSEConnectionManager<T = unknown> {
  private listeners = new Set<EventHandler<T>>();
  private running = false;
  private abort: AbortController | null = null;
  private readonly path: string;
  private readonly parse: (msg: SSEMessage) => unknown;

  constructor(
    path: string,
    parse: (msg: SSEMessage) => unknown = parseAppEvent
  ) {
    this.path = path;
    this.parse = parse;
  }

  /**
   * Subscribe to SSE events.
   * Starts connection on first subscriber, stops when last unsubscribes.
   */
  subscribe(fn: EventHandler<T>): Unsubscribe {
    this.listeners.add(fn);
    if (!this.running) {
      this.start();
    }
    return () => {
      this.listeners.delete(fn);
      if (this.listeners.size === 0) {
        this.stop();
      }
    };
  }

  /**
   * Emit event to all listeners.
   */
  private emit(evt: T): void {
    for (const fn of this.listeners) {
      try {
        fn(evt);
      } catch (error) {
        appLogger.error('transport.sse', '[SSE] Error in event handler', { error });
      }
    }
  }

  /**
   * Stop the connection.
   */
  private stop(): void {
    this.running = false;
    this.abort?.abort();
    this.abort = null;
  }

  /**
   * Start the connection with automatic reconnection.
   */
  private async start(): Promise<void> {
    this.running = true;
    this.abort = new AbortController();

    const backoff = new Backoff();

    // Ensure client is initialized (triggers API discovery in Tauri mode)
    await getClient();

    const url = `${getApiBaseUrl()}${this.path}`;

    while (this.running && this.abort && !this.abort.signal.aborted) {
      try {
        appLogger.debug('transport.sse', '[SSE] Connecting to', { url });

        for await (const msg of createSSEStream(url, {
          headers: getAuthHeaders(),
          signal: this.abort.signal,
        })) {
          // Successful receipt => reset backoff
          backoff.reset();

          const parsed = this.parse(msg);
          if (parsed !== null) {
            this.emit(parsed as T);
          }
        }

        // Stream ended gracefully
        appLogger.debug('transport.sse', '[SSE] Stream ended');
      } catch (error) {
        // Don't reconnect if explicitly stopped
        if (!this.running || this.abort?.signal.aborted) {
          break;
        }

        appLogger.error('transport.sse', '[SSE] Connection error', { error });
        const wait = backoff.next();

        appLogger.debug('transport.sse', '[SSE] Reconnecting', { waitMs: wait });

        await new Promise((resolve) => setTimeout(resolve, wait));
      }
    }

    appLogger.debug('transport.sse', '[SSE] Connection manager stopped');
  }
}

// ============================================================================
// Shared SSE connection (single fetch per app)
// ============================================================================

/**
 * Map an outer event `type` string to an `AppEventType` category.
 */
function getEventCategory(outerType: string): AppEventType | null {
  if (outerType === 'download') return 'download';
  if (outerType.startsWith('server_') || outerType === 'server_snapshot') return 'server';
  if (outerType === 'log' || outerType.startsWith('log_')) return 'log';
  if (outerType.startsWith('verification_') || outerType.startsWith('verification:')) return 'verification';
  if (outerType.startsWith('voice_')) return 'voice';
  return null;
}

/**
 * Validate a raw parsed event and optionally decode inner payloads
 * (e.g. download events with a nested `event` wrapper).
 *
 * Returns the validated event or `null` if it should be dropped.
 */
function validateEvent(data: unknown, eventType: AppEventType): unknown | null {
  if (!data || typeof data !== 'object' || !('type' in data)) {
    return null;
  }

  const outerType = (data as Record<string, unknown>).type;
  if (typeof outerType !== 'string') {
    return null;
  }

  const category = getEventCategory(outerType);
  if (category !== eventType) {
    return null;
  }

  // Download events use a wrapper format with an inner `event` field.
  if (eventType === 'download' && outerType === 'download') {
    const inner = (data as Record<string, unknown>).event;
    if (inner && typeof inner === 'object' && 'type' in inner) {
      const validated = decodeDownloadEvent(inner);
      if (validated) {
        return { type: 'download', event: validated };
      }
    }
    return null;
  }

  // All other event types pass through unmodified.
  return data;
}

/**
 * Singleton shared SSE connection manager.
 *
 * A single `fetch()` to `/api/events` is opened and kept alive for **all**
 * event types.  Previously each `SseConnection` created its own manager
 * (and therefore its own `fetch()`), consuming 3 of the browser's 6
 * HTTP/1.1 connection slots and starving API requests.
 */
let sharedManager: SSEConnectionManager | null = null;

function getSharedManager(): SSEConnectionManager {
  if (!sharedManager) {
    sharedManager = new SSEConnectionManager(SSE_EVENTS_ENDPOINT, parseAppEvent);
  }
  return sharedManager;
}

/**
 * Manages event filtering for a single `AppEventType` on top of the shared
 * `SSEConnectionManager`.  Multiple handlers can subscribe to the same type;
 * a reference count on the shared manager ensures the underlying `fetch()` is
 * opened on the first subscription and closed when the last one unsubscribes.
 */
class SseConnection<T> {
  private handlers: Set<EventHandler<T>> = new Set();
  private unsubscribe: Unsubscribe | null = null;
  private readonly eventType: AppEventType;

  constructor(eventType: AppEventType) {
    this.eventType = eventType;
  }

  /**
   * Add a handler and attach to the shared manager if this is the first.
   */
  subscribe(handler: EventHandler<T>): Unsubscribe {
    this.handlers.add(handler);

    if (!this.unsubscribe) {
      const manager = getSharedManager();
      this.unsubscribe = manager.subscribe((raw) => {
        const validated = validateEvent(raw, this.eventType);
        if (validated !== null) {
          this.broadcast(validated as T);
        }
      });
    }

    return () => {
      this.handlers.delete(handler);

      // Detach from shared manager when no more handlers for this type
      if (this.handlers.size === 0 && this.unsubscribe) {
        this.unsubscribe();
        this.unsubscribe = null;
      }
    };
  }

  /**
   * Broadcast event to all handlers for this event type.
   */
  private broadcast(data: T): void {
    for (const handler of this.handlers) {
      try {
        handler(data);
      } catch (error) {
        appLogger.error('transport.sse', '[SSE] Error in event handler', { error });
      }
    }
  }
}

/**
 * Registry of SSE connections per event type.
 */
const connections = new Map<AppEventType, SseConnection<unknown>>();

/**
 * Get or create an SSE connection for an event type.
 */
function getConnection<K extends AppEventType>(
  eventType: K
): SseConnection<AppEventMap[K]> {
  let connection = connections.get(eventType);

  if (!connection) {
    connection = new SseConnection(eventType);
    connections.set(eventType, connection);
  }

  return connection as SseConnection<AppEventMap[K]>;
}

/**
 * Subscribe to an SSE event stream.
 */
export function subscribeSseEvent<K extends AppEventType>(
  eventType: K,
  handler: EventHandler<AppEventMap[K]>
): Unsubscribe {
  const connection = getConnection(eventType);
  return connection.subscribe(handler);
}

/**
 * Create SSE-based event system.
 * Returns object with subscribe method matching EventsTransport interface.
 */
export function createSseEvents() {
  function subscribe<K extends AppEventType>(
    eventType: K,
    handler: EventHandler<AppEventMap[K]>
  ): Unsubscribe {
    return subscribeSseEvent(eventType, handler);
  }

  return { subscribe };
}
