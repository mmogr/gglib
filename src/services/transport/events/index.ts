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

/**
 * Create the EventBus for the current environment.
 */
export function createEventBus() {
  return createSseEvents();
}

export { subscribeSseEvent } from './sse';
