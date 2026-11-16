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
import { readSse, type SseEvent } from '../../../utils/sse';
import { apiFetch } from '../api/client';
import { renewAfterRefusal } from '../api/renew';
import { getEventCategory } from './category';
import { Backoff } from './backoff';
import { OpenSignal } from './open';

/**
 * Unified SSE endpoint path.
 *
 * All events (server, download, model, verification, proxy) multiplex through one
 * connection here. No `log` family: those stream from `/api/servers/{port}/logs/stream`.
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
function parseAppEvent(msg: SseEvent): unknown {
  const data = safeJsonParse(msg.data);
  // Backend uses default event type, payload is in data
  return data;
}

/**
 * One run of the reconnect loop: what it is doing now, and how to wake it.
 *
 * Each start of the loop has its own, so a loop that was stopped, still
 * settling its last attempt, can only change its own state and never the
 * state of the loop that replaced it (a stop and a start in one tick, as
 * StrictMode remounts do).
 */
interface Loop {
  readonly abort: AbortController;
  /**
   * `attempting` from the request until whatever follows its refusal is
   * done, a credential's renewal included; `connected` while a stream is
   * read; `waiting` in a reconnect wait.
   */
  phase: 'attempting' | 'connected' | 'waiting';
  /** A wake asked while attempting: the next attempt is made at once. */
  wakeAsked: boolean;
  /** Ends the reconnect wait early, while one is running. */
  wake: (() => void) | null;
}

/**
 * SSE connection manager with automatic reconnection.
 * Supports multiple subscribers sharing a single connection.
 */
export class SSEConnectionManager<T = unknown> {
  private listeners = new Set<EventHandler<T>>();
  /** Announced each time the stream opens, reconnections included. */
  readonly opened = new OpenSignal();
  /** Announced each time an open stream ends: closed, failed or stopped. */
  readonly closed = new OpenSignal();
  private running = false;
  /** The loop that is running, if one is. */
  private loop: Loop | null = null;
  private readonly path: string;
  private readonly parse: (msg: SseEvent) => unknown;

  constructor(
    path: string,
    parse: (msg: SseEvent) => unknown = parseAppEvent
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
    const loop = this.loop;
    this.loop = null;
    if (loop) this.enter(loop, 'waiting');
    loop?.abort.abort();
    loop?.wake?.();
  }

  /** Whether a stream is open now. */
  get isOpen(): boolean {
    return this.loop?.phase === 'connected';
  }

  /**
   * Move `loop` to `phase`, telling the listeners when that ends an open
   * stream. A stopped loop is told nothing more: its stop said it.
   */
  private enter(loop: Loop, phase: Loop['phase']): void {
    const ended = loop.phase === 'connected' && phase !== 'connected';
    loop.phase = phase;
    if (ended && !loop.abort.signal.aborted) this.closed.announce();
  }

  /**
   * Cut a reconnect wait short and connect now: for a reader that knows the
   * daemon is back, so its stream need not sleep out a backoff that grew
   * while the daemon was gone. Asked while an attempt is out (or while the
   * credential it was refused with is renewed), it is kept: that attempt,
   * begun before the daemon listened, may yet fail, and the next is then
   * made at once. Asked while connected, the connection is kept and its
   * opening announced again: a reader that let its state go when the
   * daemon seemed gone (a health probe that failed while the stream stayed
   * up) reads it again, as on any opening. Nothing happens while stopped.
   */
  retryNow(): void {
    const loop = this.loop;
    if (!loop) return;
    if (loop.wake) loop.wake();
    else if (loop.phase === 'attempting') loop.wakeAsked = true;
    else if (loop.phase === 'connected') this.opened.announce();
  }

  /**
   * Start the connection with automatic reconnection.
   */
  private async start(): Promise<void> {
    this.running = true;
    // Each start owns its loop, controller and all, so a loop stopped while
    // it slept in a reconnect wait ends when it wakes, whatever a later
    // start set going: one stream, however often the readers come and go.
    const loop: Loop = { abort: new AbortController(), phase: 'attempting', wakeAsked: false, wake: null };
    this.loop = loop;
    const { signal } = loop.abort;

    const backoff = new Backoff();
    // Whether the last attempt was refused for its credential, which was
    // then renewed: one at once after that, not a run of them.
    let renewedLast = false;

    while (!signal.aborted) {
      this.enter(loop, 'attempting');
      loop.wakeAsked = false;
      try {
        appLogger.debug('transport.sse', '[SSE] Connecting to', { path: this.path });

        // The daemon's base URL and the session's token are resolved here, on
        // each connection, so one made after a renewal presents the new token.
        const response = await apiFetch(this.path, { signal, stream: true });
        renewedLast = false;
        this.enter(loop, 'connected');
        this.opened.announce();

        for await (const msg of readSse(response)) {
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
        if (signal.aborted) {
          break;
        }

        // Still the attempt until the renewal below is done: a wake asked
        // meanwhile is kept.
        this.enter(loop, 'attempting');
        appLogger.error('transport.sse', '[SSE] Connection error', { error });
        // A restarted daemon minted a new token. A refusal for the old one
        // says the daemon answers, so the attempt with the new one is made
        // at once rather than after a wait that grew while it was gone.
        const renewed = await renewAfterRefusal(error).catch(() => false);
        const again = (renewed && !renewedLast) || loop.wakeAsked;
        renewedLast = renewed;
        if (signal.aborted) break;
        if (again) continue;

        const wait = backoff.next();
        appLogger.debug('transport.sse', '[SSE] Reconnecting', { waitMs: wait });

        this.enter(loop, 'waiting');
        await new Promise<void>((resolve) => {
          const timer = setTimeout(resolve, wait);
          loop.wake = () => {
            clearTimeout(timer);
            resolve();
          };
        });
        loop.wake = null;
      }
    }

    appLogger.debug('transport.sse', '[SSE] Connection manager stopped');
  }
}

// ============================================================================
// Shared SSE connection (single fetch per app)
// ============================================================================

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
 * Be told each time the shared stream opens, reconnections included.
 */
export function onEventStreamOpen(handler: () => void): Unsubscribe {
  return getSharedManager().opened.listen(handler);
}

/** Be told each time an open stream ends: closed by the daemon, failed, or stopped. */
export function onEventStreamClose(handler: () => void): Unsubscribe {
  return getSharedManager().closed.listen(handler);
}

/** Whether the shared stream is open now. */
export function isEventStreamOpen(): boolean {
  return sharedManager?.isOpen ?? false;
}

/**
 * Connect the shared stream now if it is waiting to reconnect: for a reader
 * that has just heard the daemon come back.
 */
export function reconnectEventStreamNow(): void {
  sharedManager?.retryNow();
}
