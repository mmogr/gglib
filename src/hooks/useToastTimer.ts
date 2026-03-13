import { useEffect, useRef, useState } from 'react';

const EXIT_ANIMATION_MS = 300;

/**
 * Manages a toast auto-dismiss timer with pause/resume support.
 *
 * - Starts counting down from `duration` on mount.
 * - `pause()` freezes the countdown (e.g. on hover or focus).
 * - `resume()` continues from where it left off.
 * - `isExiting` becomes true 300ms before expiry so the exit animation plays.
 */
export function useToastTimer(duration: number, onExpire: () => void) {
  const [isExiting, setIsExiting] = useState(false);

  const onExpireRef = useRef(onExpire);
  onExpireRef.current = onExpire;

  const remainingRef = useRef(duration);
  const startedAtRef = useRef<number | null>(null);
  const exitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const removeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimers = () => {
    if (exitTimerRef.current !== null) clearTimeout(exitTimerRef.current);
    if (removeTimerRef.current !== null) clearTimeout(removeTimerRef.current);
  };

  const start = (ms: number) => {
    clearTimers();
    remainingRef.current = ms;
    startedAtRef.current = Date.now();
    exitTimerRef.current = setTimeout(() => setIsExiting(true), ms - EXIT_ANIMATION_MS);
    removeTimerRef.current = setTimeout(() => onExpireRef.current(), ms);
  };

  const pause = () => {
    if (startedAtRef.current === null) return;
    const elapsed = Date.now() - startedAtRef.current;
    remainingRef.current = Math.max(EXIT_ANIMATION_MS, remainingRef.current - elapsed);
    startedAtRef.current = null;
    clearTimers();
  };

  const resume = () => {
    if (startedAtRef.current !== null) return; // already running
    start(remainingRef.current);
  };

  useEffect(() => {
    start(duration);
    return clearTimers;
    // Intentionally only runs on mount; duration changes are not expected.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { isExiting, setIsExiting, pause, resume };
}
