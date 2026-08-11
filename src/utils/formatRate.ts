/** Compact per-second rate figure: sub-10 keeps one decimal, larger values round whole. */
export function formatRate(perSecond: number): string {
  return perSecond < 10 ? perSecond.toFixed(1) : String(Math.round(perSecond));
}
