/**
 * How full a context is, as every meter says it: one whole-number percent
 * and one severity. A ring's colour, its figure and its words all come from
 * these two, so an exact half cannot colour a ring one way and word it
 * another.
 *
 * The arithmetic is the rule of `contracts/context/readings.json`, the worked
 * examples every gglib client draws its context reading from.
 *
 * @module contextUsage
 */

export type UsageSeverity = 'normal' | 'warning' | 'danger';

/**
 * `used` of `size` as a whole percent: a half rounds up, and it is never
 * over 100. Worked in whole numbers, so every exact half rounds the same
 * way: 113 of 200 is 57, where `Math.round((113 / 200) * 100)` is 56,
 * because the float product is 56.49999999999999.
 */
export function usagePercent(used: number, size: number): number {
  return Math.min(100, Math.floor((200 * used + size) / (2 * size)));
}

/** Under 70 a meter is plain; from 70 it warns; from 90 it is almost full. */
export function usageSeverity(percent: number): UsageSeverity {
  return percent >= 90 ? 'danger' : percent >= 70 ? 'warning' : 'normal';
}
