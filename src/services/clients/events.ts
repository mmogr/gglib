/**
 * Events Client
 *
 * Thin wrapper for Transport event subscriptions.
 * Delegates to getTransport() for platform-agnostic event handling.
 */

import { getTransport } from '../transport';
import type { Unsubscribe } from '../transport/types/common';
import type { AppEventType, AppEventMap } from '../transport/types/events';

/**
 * Subscribe to an event stream.
 *
 * @param type - The event type to subscribe to ('server', 'download', 'log')
 * @param handler - Callback invoked with each event payload
 * @returns Unsubscribe function to stop receiving events
 */
export function subscribeToEvent<K extends AppEventType>(
  type: K,
  handler: (payload: AppEventMap[K]) => void
): Unsubscribe {
  return getTransport().subscribe(type, handler);
}
