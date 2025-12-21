/**
 * Settings API module.
 * Handles application settings CRUD.
 */

import { get, put } from './client';
import type { AppSettings, UpdateSettingsRequest } from '../types/settings';

/**
 * Get current application settings.
 */
export async function getSettings(): Promise<AppSettings> {
  return get<AppSettings>('/api/settings');
}

/**
 * Update application settings (partial update).
 */
export async function updateSettings(settings: UpdateSettingsRequest): Promise<AppSettings> {
  return put<AppSettings>('/api/settings', settings);
}
