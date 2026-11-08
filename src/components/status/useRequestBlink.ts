/**
 * The endpoint dot's once-per-request blink, the only motion at rest
 * (styles README, "Motion"): on for 150 ms each time the proxy's
 * `total_requests` grows between two frames of its dashboard stream. At
 * most once a second; never for the first frame after the stream was idle
 * (a count that rose meanwhile is not a request seen), nor for a restarted
 * proxy's lower count; never under reduced motion. The sidebar's endpoint
 * line and the tray's Endpoint block both draw it, through `StatusMark`'s
 * `blink`.
 *
 * @module components/status/useRequestBlink
 */

import { useEffect, useRef, useState } from 'react';

/** How long the dot's blink lasts. */
export const BLINK_MS = 150;
/** The least time between two blinks. */
const BLINK_GAP_MS = 1000;

/**
 * Whether a frame counting `served` requests blinks the dot: only when the
 * count grew since the frame before (`seen`, `null` for none), and not
 * within a second of the last blink (`blinkedAt`).
 */
export function shouldBlink(seen: number | null, served: number, blinkedAt: number | null, now: number): boolean {
  if (seen === null || served <= seen) return false;
  return blinkedAt === null || now - blinkedAt >= BLINK_GAP_MS;
}

function reducedMotion(): boolean {
  return typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Whether the dot is blinking now, for the proxy's request count (`null` while its stream is idle). */
export function useRequestBlink(served: number | null): boolean {
  const [blinking, setBlinking] = useState(false);
  const seen = useRef<number | null>(null);
  const blinkedAt = useRef<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  useEffect(() => {
    const before = seen.current;
    // Idle forgets the count: the first frame after it is a baseline again.
    seen.current = served;
    const now = Date.now();
    if (served === null || !shouldBlink(before, served, blinkedAt.current, now) || reducedMotion()) return;
    blinkedAt.current = now;
    setBlinking(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setBlinking(false), BLINK_MS);
  }, [served]);
  return blinking;
}
