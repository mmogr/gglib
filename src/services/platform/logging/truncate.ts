/**
 * Payload truncation utilities for logging.
 *
 * Deep-truncates string values and caps arrays to prevent memory/IPC bloat
 * in log transports.
 */

const MAX_PAYLOAD_STRING_LENGTH = 500;

/**
 * Truncate a string to max length with ellipsis.
 */
export function truncateString(str: string, maxLength = MAX_PAYLOAD_STRING_LENGTH): string {
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength - 3) + '...';
}

/**
 * Deep truncate all string values in an object.
 * Arrays are capped at 10 items.
 */
export function truncatePayload(
  obj: unknown,
  maxStringLength = MAX_PAYLOAD_STRING_LENGTH,
  depth = 0,
): unknown {
  // Prevent infinite recursion
  if (depth > 5) return '[max depth]';

  if (obj === null || obj === undefined) return obj;

  if (typeof obj === 'string') {
    return truncateString(obj, maxStringLength);
  }

  if (Array.isArray(obj)) {
    const truncatedArray = obj
      .slice(0, 10)
      .map((item) => truncatePayload(item, maxStringLength, depth + 1));
    if (obj.length > 10) {
      truncatedArray.push(`[...${obj.length - 10} more items]`);
    }
    return truncatedArray;
  }

  if (typeof obj === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
      result[key] = truncatePayload(value, maxStringLength, depth + 1);
    }
    return result;
  }

  return obj;
}
