/**
 * Type-safe object composition helpers.
 * Provides compile-time collision detection for object spreads.
 */

import { appLogger } from '../platform';

/**
 * Type that ensures A and B have no overlapping keys.
 * If they overlap, B becomes `never`, preventing the merge.
 */
type NoOverlap<A, B> = keyof A & keyof B extends never ? B : never;

/**
 * Merge two objects with compile-time collision detection.
 * 
 * If A and B have any keys in common, TypeScript will error.
 * This prevents silent overwrites from object spreads.
 * 
 * @example
 * ```ts
 * const api = { listModels: () => {} };
 * const platform = { openUrl: () => {} };
 * const merged = mergeNoOverlap(api, platform); // OK
 * 
 * const duplicate = { listModels: () => {} };
 * const bad = mergeNoOverlap(api, duplicate); // Compile error!
 * ```
 */
export function mergeNoOverlap<A extends object, B extends object>(
  a: A,
  b: NoOverlap<A, B> & B
): A & B {
  return { ...a, ...b };
}

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
