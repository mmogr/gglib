/** Shared formatting helpers for the benchmark surfaces. */

export function formatTps(tps: number | null | undefined): string {
  if (tps == null) return '—';
  return `${tps.toFixed(1)} t/s`;
}

export function formatMs(ms: number | null | undefined): string {
  if (ms == null) return '—';
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms.toFixed(0)} ms`;
}

export function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}


