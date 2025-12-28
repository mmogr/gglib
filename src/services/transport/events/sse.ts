/**
 * SSE (Server-Sent Events) handling for transport layer.
 * 
 * Uses fetch-based streaming with Bearer token authentication.
 * Provides reference-counted connection management for multi-subscriber safety.
 */

import type { Unsubscribe, EventHandler } from '../types/common';
import type { AppEventType, AppEventMap } from '../types/events';
import { decodeDownloadEvent } from '../../decoders/downloadEvent';
import { createSSEStream, type SSEMessage } from '../../../utils/sse';
import { getApiBaseUrl, getAuthHeaders, getClient } from '../api/client';

/**
 * Unified SSE endpoint path.
 * 
 * All events (server, download, log) are multiplexed through a single
 * SSE connection at this endpoint for efficiency.
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
        console.error('[SSE] Error in event handler:', error);
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
        if (import.meta.env.DEV) {
          console.debug('[SSE] Connecting to:', url);
        }

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
        if (import.meta.env.DEV) {
          console.debug('[SSE] Stream ended');
        }
      } catch (error) {
        // Don't reconnect if explicitly stopped
        if (!this.running || this.abort?.signal.aborted) {
          break;
        }

        console.error('[SSE] Connection error:', error);
        const wait = backoff.next();
        
        if (import.meta.env.DEV) {
          console.debug(`[SSE] Reconnecting in ${wait}ms...`);
        }

        await new Promise((resolve) => setTimeout(resolve, wait));
      }
    }

    if (import.meta.env.DEV) {
      console.debug('[SSE] Connection manager stopped');
    }
  }
}

/**
 * Manages a reference-counted SSE connection with event filtering.
 * Multiple subscribers can share a single connection.
 */
class SseConnection<T> {
  private manager: SSEConnectionManager;
  private handlers: Set<EventHandler<T>> = new Set();
  private unsubscribe: Unsubscribe | null = null;
  private readonly eventType: AppEventType;

  constructor(path: string, eventType: AppEventType) {
    this.eventType = eventType;
    this.manager = new SSEConnectionManager(path, (msg) => this.parseAndFilter(msg));
  }

  /**
   * Add a handler and connect if this is the first subscriber.
   */
  subscribe(handler: EventHandler<T>): Unsubscribe {
    this.handlers.add(handler);

    if (!this.unsubscribe) {
      this.unsubscribe = this.manager.subscribe((event) => {
        this.broadcast(event as T);
      });
    }

    return () => {
      this.handlers.delete(handler);
      
      // Disconnect if no more handlers
      if (this.handlers.size === 0 && this.unsubscribe) {
        this.unsubscribe();
        this.unsubscribe = null;
      }
    };
  }

  /**
   * Parse and filter events based on type.
   */
  private parseAndFilter(msg: SSEMessage): unknown | null {
    const data = parseAppEvent(msg);
    return this.validateEvent(data);
  }

  /**
   * Validate and decode events based on type.
   * 
   * Events are wrapped in an AppEvent envelope with \`type\` field indicating
   * the category ('download', 'server', 'log', etc.). Only broadcast events
   * that match this connection's event type.
   */
  private validateEvent(data: unknown): unknown | null {
    if (!data || typeof data !== 'object' || !('type' in data)) {
      return null;
    }

    const outerType = (data as Record<string, unknown>).type;
    if (typeof outerType !== 'string') {
      return null;
    }

    // Filter by event type - only process events matching this connection's type
    const eventCategory = this.getEventCategory(outerType);
    if (eventCategory !== this.eventType) {
      return null; // Not for this connection
    }

    // For download events with wrapper format, validate the inner event
    if (this.eventType === 'download' && outerType === 'download') {
      const inner = (data as Record<string, unknown>).event;
      if (inner && typeof inner === 'object' && 'type' in inner) {
        // Validate inner download event
        const validated = decodeDownloadEvent(inner);
        if (validated) {
          // Return the full wrapper with validated inner event
          return { type: 'download', event: validated };
        }
      }
      return null;
    }

    // Pass through other events without modification
    return data;
  }

  /**
   * Map an outer event type to an AppEventType category.
   */
  private getEventCategory(outerType: string): AppEventType | null {
    // Download events use wrapper format
    if (outerType === 'download') return 'download';
    // Server events have specific prefixes
    if (outerType.startsWith('server_') || outerType === 'server_snapshot') return 'server';
    // Log events
    if (outerType === 'log' || outerType.startsWith('log_')) return 'log';
    return null;
  }

  /**
   * Broadcast event to all handlers.
   */
  private broadcast(data: T): void {
    for (const handler of this.handlers) {
      try {
        handler(data);
      } catch (error) {
        console.error('[SSE] Error in event handler:', error);
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
    connection = new SseConnection(SSE_EVENTS_ENDPOINT, eventType);
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
 * Parse server event from SSE payload.
 */
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
