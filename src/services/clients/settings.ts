/**
 * Settings client module.
 *
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 *
 * @module services/clients/settings
 */

import { getTransport } from '../transport';
import type { AppSettings, UpdateSettingsRequest } from '../../types';

/**
 * Get current application settings.
 */
export async function getSettings(): Promise<AppSettings> {
  return getTransport().getSettings();
}

/**
 * Update application settings. Only provided fields are updated.
 * Returns the updated settings.
 */
export async function updateSettings(settings: UpdateSettingsRequest): Promise<AppSettings> {
  return getTransport().updateSettings(settings);
}
