/**
 * SSE (Server-Sent Events) handling for transport layer.
 * Provides reference-counted connection management for multi-subscriber safety.
 */

import type { Unsubscribe, EventHandler } from '../types/common';
import type { AppEventType, AppEventMap, ServerEvent, DownloadEvent, LogEvent } from '../types/events';
import { decodeDownloadEvent } from '../../decoders/downloadEvent';

/**
 * Unified SSE endpoint path.
 * 
 * All events (server, download, log) are multiplexed through a single
 * SSE connection at this endpoint for efficiency.
 */
export const SSE_EVENTS_ENDPOINT = '/api/events';

/**
 * SSE endpoint paths for each event type.
 * All currently point to the unified endpoint.
 */
const SSE_ENDPOINTS: Record<AppEventType, string> = {
  'server': SSE_EVENTS_ENDPOINT,
  'download': SSE_EVENTS_ENDPOINT,
  'log': SSE_EVENTS_ENDPOINT,
};

/**
 * Manages a reference-counted SSE connection.
 * Multiple subscribers can share a single connection.
 */
class SseConnection<T> {
  private eventSource: EventSource | null = null;
  private handlers: Set<EventHandler<T>> = new Set();
  private readonly endpoint: string;
  private readonly baseUrl: string;
  private readonly eventType: AppEventType;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;

  constructor(endpoint: string, baseUrl: string = '', eventType: AppEventType) {
    this.endpoint = endpoint;
    this.baseUrl = baseUrl;
    this.eventType = eventType;
  }

  /**
   * Add a handler and connect if this is the first subscriber.
   */
  subscribe(handler: EventHandler<T>): Unsubscribe {
    this.handlers.add(handler);

    if (!this.eventSource) {
      this.connect();
    }

    return () => {
      this.handlers.delete(handler);
      
      // Disconnect if no more handlers
      if (this.handlers.size === 0) {
        this.disconnect();
      }
    };
  }

  private connect(): void {
    const url = `${this.baseUrl}${this.endpoint}`;
    this.eventSource = new EventSource(url);

    this.eventSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        // Validate and decode the event if it's a download event
        // (For download events, we apply runtime validation)
        const validated = this.validateEvent(data);
        if (validated !== null) {
          this.broadcast(validated as T);
        }
      } catch (error) {
        console.error('Failed to parse SSE message:', error);
      }
    };

    this.eventSource.onerror = (error) => {
      console.error('SSE connection error:', error);
      this.scheduleReconnect();
    };
  }

  /**
   * Validate and decode events based on type.
   * 
   * Events are wrapped in an AppEvent envelope with `type` field indicating
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
    // Map outer type to AppEventType: 'download' -> 'download', 'server_*' -> 'server', etc.
    const eventCategory = this.getEventCategory(outerType);
    if (eventCategory !== this.eventType) {
      return null; // Not for this connection
    }

    // For download events with wrapper format, validate the inner event
    if (this.eventType === 'download' && outerType === 'download') {
      const inner = (data as Record<string, unknown>).event;
      if (inner && typeof inner === 'object' && 'type' in inner) {
        const innerType = (inner as Record<string, unknown>).type;
        if (typeof innerType === 'string') {
          // Validate inner download event
          const validated = decodeDownloadEvent(inner);
          if (validated) {
            // Return the full wrapper with validated inner event
            return { type: 'download', event: validated };
          }
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

  private disconnect(): void {
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }

  private scheduleReconnect(): void {
    if (this.handlers.size === 0) return;

    this.disconnect();
    
    // Reconnect after 3 seconds
    this.reconnectTimeout = setTimeout(() => {
      if (this.handlers.size > 0) {
        this.connect();
      }
    }, 3000);
  }

  private broadcast(data: T): void {
    for (const handler of this.handlers) {
      try {
        handler(data);
      } catch (error) {
        console.error('Error in SSE event handler:', error);
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
  eventType: K,
  baseUrl: string = ''
): SseConnection<AppEventMap[K]> {
  let connection = connections.get(eventType);
  
  if (!connection) {
    const endpoint = SSE_ENDPOINTS[eventType];
    connection = new SseConnection(endpoint, baseUrl, eventType);
    connections.set(eventType, connection);
  }

  return connection as SseConnection<AppEventMap[K]>;
}

/**
 * Subscribe to an SSE event stream.
 */
export function subscribeSseEvent<K extends AppEventType>(
  eventType: K,
  handler: EventHandler<AppEventMap[K]>,
  baseUrl: string = ''
): Unsubscribe {
  const connection = getConnection(eventType, baseUrl);
  return connection.subscribe(handler);
}

/**
 * Parse server event from SSE payload.
 */
export function parseServerEvent(payload: unknown): ServerEvent {
  return payload as ServerEvent;
}

/**
 * Parse download event from SSE payload.
 */
export function parseDownloadEvent(payload: unknown): DownloadEvent {
  return payload as DownloadEvent;
}

/**
 * Parse log event from SSE payload.
 */
export function parseLogEvent(payload: unknown): LogEvent {
  return payload as LogEvent;
}
