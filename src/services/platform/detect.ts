/**
 * Platform detection utilities
 * TRANSPORT_EXCEPTION: This module is the canonical source for platform detection.
 * UI code should import isDesktop() from 'services/platform' rather than checking isTauriApp directly.
 */

import { isTauriApp } from '../../utils/platform';

/**
 * Returns true if running in the Tauri desktop app.
 * UI components should use this instead of importing isTauriApp directly.
 */
export function isDesktop(): boolean {
  return isTauriApp;
}

/**
 * Returns true if running in web browser mode.
 */
export function isWeb(): boolean {
  return !isTauriApp;
}
