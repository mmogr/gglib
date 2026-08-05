/**
 * EventBus factory.
 *
 * One implementation for every mode: SSE from the backend's /api/events
 * stream. The backend is the gglib daemon in both desktop and web — the
 * desktop WebView discovers its base URL via get_embedded_api_info and then
 * consumes the same stream a browser tab does, so there is no Tauri-event
 * branch left to maintain.
 */

import { createSseEvents } from './sse';
import type { EventsTransport } from '../types/events';

/**
 * Create EventBus for the current environment.
 * Returns object matching EventsTransport interface.
 */
export function createEventBus(): EventsTransport {
  return createSseEvents();
}

export { subscribeSseEvent } from './sse';
