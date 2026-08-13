/**
 * Type-safe object composition helpers.
 * Provides compile-time collision detection for object spreads.
 */

import { appLogger } from '../platform';

/**
 * Check for duplicate keys at runtime (dev mode only).
 * Logs error to console if collisions are detected.
 * 
 * @param objects - Objects to check for key collisions
 * @returns true if collisions found, false otherwise
 */
export function checkCollisions(...objects: object[]): boolean {
  const allKeys = new Map<string, number>();
  const collisions: string[] = [];
  
  for (const obj of objects) {
    for (const key of Object.keys(obj)) {
      const count = allKeys.get(key) || 0;
      allKeys.set(key, count + 1);
      
      if (count === 1) {
        collisions.push(key);
      }
    }
  }
  
  if (collisions.length > 0) {
    appLogger.error('transport.util', 'Key collisions detected in transport composition', { collisions });
    appLogger.error('transport.util', 'This indicates duplicate method names across modules');
    return true;
  }
  
  return false;
}
