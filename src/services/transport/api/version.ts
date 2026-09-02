/**
 * Build identity API module.
 * Reports which build of gglib the daemon answering us is running.
 */

import { VERSION_PATH } from '../../api/routes';
import { get } from './client';
import type { VersionDto } from '../../../types/generated/VersionDto';

/**
 * Get the daemon's build identity.
 *
 * Read from the daemon rather than from `package.json` so the dashboard
 * reports the build actually serving it — a GUI talking to a daemon left
 * running from an older install should say so, not repeat what it was
 * bundled with.
 */
export async function getVersion(): Promise<VersionDto> {
  return get<VersionDto>(VERSION_PATH);
}
