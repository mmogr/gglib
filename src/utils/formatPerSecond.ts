/**
 * Compact per-second count: sub-10 keeps one decimal, larger values round whole.
 *
 * A bare figure with no unit — the caller supplies "tok/s", "req/s" or
 * whatever it is counting. Named `formatRate` until a second `formatRate` in
 * `utils/format.ts` turned out to format *bytes* per second, complete with a
 * "MB/s" suffix. Two functions, one name, incompatible units, chosen by import
 * path: a one-character typo silently changed what a number meant.
 */
export function formatPerSecond(perSecond: number): string {
  return perSecond < 10 ? perSecond.toFixed(1) : String(Math.round(perSecond));
}
