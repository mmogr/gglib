/**
 * API transport module.
 * Combines all domain API modules into a single transport object.
 */

import { createModelsApi } from './models';
import * as tags from './tags';
import * as settings from './settings';
import * as servers from './servers';
import * as proxy from './proxy';
import * as downloads from './downloads';
import * as mcp from './mcp';
import * as chat from './chat';
import * as verification from './verification';

/**
 * Create unified API transport.
 * Returns a plain object with all API methods.
 */
export function createApiTransport() {
  return {
    ...createModelsApi(),
    ...tags,
    ...settings,
    ...servers,
    ...proxy,
    ...downloads,
    ...mcp,
    ...chat,
    ...verification,
  };
}
