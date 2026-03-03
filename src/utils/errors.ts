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
