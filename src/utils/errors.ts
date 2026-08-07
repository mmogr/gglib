/**
 * Type-safe AbortError predicate.
 *
 * `fetch()` and `ReadableStream.read()` both throw a `DOMException` with
 * `name === 'AbortError'` when an `AbortSignal` fires.  `DOMException`
 * implements `Error` in all modern environments, so an `instanceof Error`
 * guard works — but the cast pattern `(err as Error).name` does not guard
 * the type at all and silently passes through non-Error rejections.  This
 * predicate uses `instanceof Error` as the structural guard and then checks
 * the discriminating `name` property.
 */
export function isAbortError(err: unknown): err is DOMException {
  return err instanceof Error && err.name === 'AbortError';
}

/**
 * Render an unknown thrown value as a human-readable message.
 *
 * Template-interpolating a caught value (`` `Failed: ${err}` ``) prints
 * "[object Object]" for anything that isn't an Error or string — use this
 * instead wherever an error reaches user-facing text or log messages.
 */
export function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}
