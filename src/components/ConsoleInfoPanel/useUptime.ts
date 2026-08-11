import { useEffect, useRef, useState } from 'react';

/** Format an uptime duration in a human-readable way. */
const formatUptime = (startTime: number): string => {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - startTime;

  if (diff < 60) {
    return `${diff}s`;
  } else if (diff < 3600) {
    const mins = Math.floor(diff / 60);
    const secs = diff % 60;
    return `${mins}m ${secs}s`;
  } else {
    const hours = Math.floor(diff / 3600);
    const mins = Math.floor((diff % 3600) / 60);
    return `${hours}h ${mins}m`;
  }
};

/**
 * Live uptime string for a server started at `startTime` (unix seconds).
 * Updates are synced to the wall-clock second boundary to prevent flicker.
 */
export function useUptime(startTime: number): string {
  const [uptime, setUptime] = useState(() => formatUptime(startTime));
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
    }

    setUptime(formatUptime(startTime));

    const msUntilNextSecond = 1000 - (Date.now() % 1000);
    const timeout = setTimeout(() => {
      setUptime(formatUptime(startTime));
      intervalRef.current = setInterval(() => {
        setUptime(formatUptime(startTime));
      }, 1000);
    }, msUntilNextSecond);

    return () => {
      clearTimeout(timeout);
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [startTime]);

  return uptime;
}
