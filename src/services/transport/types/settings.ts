/**
 * Settings transport sub-interface.
 * Handles application settings CRUD.
 */

import type { AppSettings, UpdateSettingsRequest } from '../../../types';

// Re-export existing types
export type { AppSettings, UpdateSettingsRequest };

/**
 * Settings transport operations.
 */
export interface SettingsTransport {
  /** Get current application settings. */
  getSettings(): Promise<AppSettings>;

  /** Update application settings. Only provided fields are updated. */
  updateSettings(settings: UpdateSettingsRequest): Promise<AppSettings>;
}
