import { useEffect, useState } from 'react';

/**
 * Shared ticker for live timer updates across multiple components.
 * Only runs when enabled, preventing unnecessary re-renders when not streaming.
 * 
 * @param enabled - Whether the ticker should be active
 * @param intervalMs - Interval between ticks in milliseconds (default: 100ms)
 * @returns Current tick count (increments while enabled)
 */
export function useSharedTicker(enabled: boolean, intervalMs = 100): number {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    if (!enabled) return;
    const id = setInterval(() => setTick((t) => t + 1), intervalMs);
    return () => clearInterval(id);
  }, [enabled, intervalMs]);

  return tick;
}
