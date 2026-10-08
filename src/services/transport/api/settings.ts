/**
 * Settings API module.
 * Handles application settings CRUD.
 */

import { get, post, put } from './client';
import type { AppSettings, UpdateSettingsRequest } from '../../../types';
import type { InstalledTemplates } from '../../../types/generated/InstalledTemplates';

/**
 * Get current application settings.
 */
export async function getSettings(): Promise<AppSettings> {
  return get<AppSettings>('/api/config/settings');
}

/**
 * Update application settings (partial update).
 */
export async function updateSettings(settings: UpdateSettingsRequest): Promise<AppSettings> {
  return put<AppSettings>('/api/config/settings', settings);
}

/**
 * Add the starter profiles (`gglib config profile install-templates`). A
 * profile already stored under one of their names is kept as it is, and
 * named in the answer's `kept`.
 */
export async function installProfileTemplates(): Promise<InstalledTemplates> {
  return post<InstalledTemplates>('/api/config/profiles/install-templates');
}
