/**
 * Tauri event handling for transport layer.
 * Wraps @tauri-apps/api/event with typed handlers.
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import type { Unsubscribe, EventHandler } from '../types/common';
import type { AppEventType, AppEventMap, ServerEvent, DownloadEvent, LogEvent } from '../types/events';
import {
  DOWNLOAD_EVENT_NAMES,
  SERVER_EVENT_NAMES,
  LOG_EVENT_NAMES,
} from './eventNames';

const eventModulePromise = import('@tauri-apps/api/event');

/**
 * Map AppEventType to the set of granular event names from the backend.
 * Each event type subscribes to multiple specific events (e.g., download:started, download:progress).
 */
const TAURI_EVENT_NAMES: Record<AppEventType, readonly string[]> = {
  'server': SERVER_EVENT_NAMES,
  'download': DOWNLOAD_EVENT_NAMES,
  'log': LOG_EVENT_NAMES,
};

/**
 * Create a subscription to a Tauri event type.
 * 
 * This function subscribes to all granular event names for the given event type
 * (e.g., 'download' subscribes to 'download:started', 'download:progress', etc.)
 * and routes all events to the same handler.
 * 
 * Cancel-safe: if unsubscribe is called before listen resolves, cleanup happens automatically.
 */
export function subscribeTauriEvent<K extends AppEventType>(
  eventType: K,
  handler: EventHandler<AppEventMap[K]>
): Unsubscribe {
  const eventNames = TAURI_EVENT_NAMES[eventType];
  const unlistenFns: UnlistenFn[] = [];
  let cancelled = false;

  // Subscribe to each granular event name
  const setupPromises = eventNames.map((eventName) =>
    eventModulePromise
      .then(({ listen }) =>
        listen<AppEventMap[K]>(eventName, (event) => {
          if (!cancelled) {
            handler(event.payload);
          }
        }).then((fn) => {
          if (cancelled) {
            // Already unsubscribed before listener was ready - clean up immediately
            fn();
          } else {
            unlistenFns.push(fn);
          }
        })
      )
      .catch((error) => {
        console.error(`Failed to subscribe to Tauri event ${eventName}:`, error);
      })
  );

  // Wait for all listeners to be set up
  Promise.all(setupPromises).catch((error) => {
    console.error(`Failed to set up Tauri event listeners for ${eventType}:`, error);
  });

  // Return unsubscribe function that cleans up all listeners
  return () => {
    cancelled = true;
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    unlistenFns.length = 0;
  };
}

/**
 * Create Tauri-based event system.
 * Returns object with subscribe method matching EventsTransport interface.
 */
export function createTauriEvents() {
  function subscribe<K extends AppEventType>(
    eventType: K,
    handler: EventHandler<AppEventMap[K]>
  ): Unsubscribe {
    return subscribeTauriEvent(eventType, handler);
  }
  
  return { subscribe };
}

/**
 * Parse server event from Tauri payload.
 */
export function parseServerEvent(payload: unknown): ServerEvent {
  // Tauri sends events directly as the correct shape
  return payload as ServerEvent;
}

/**
 * Parse download event from Tauri payload.
 */
export function parseDownloadEvent(payload: unknown): DownloadEvent {
  return payload as DownloadEvent;
}

/**
 * Parse log event from Tauri payload.
 */
export function parseLogEvent(payload: unknown): LogEvent {
  return payload as LogEvent;
}
