/**
 * Built-in tools index.
 * Registers all built-in tools with the global registry.
 */

import { getToolRegistry } from '../registry';

// Import built-in tools
import * as time from './time';

/**
 * Register all built-in tools with the given registry.
 * Called automatically when importing from the tools index.
 */
export function registerBuiltinTools(): void {
  const registry = getToolRegistry();

  // Only register if not already registered (idempotent)
  if (!registry.has('get_current_time')) {
    registry.register(time.definition, time.execute);
  }
}

// Export individual tools for direct access if needed
export { time };
