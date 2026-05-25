/**
 * WaveScrubber — Phase M rewind component.
 *
 * Renders a horizontal list of completed wave waypoints derived from
 * `wave_completed` events in the run's event log.  Clicking a waypoint
 * triggers a destructive confirmation dialog before calling `onRewind`.
 *
 * HTTP-only transport — no tauri::command.
 */

import { FC, useState } from 'react';
import { RotateCcw, ChevronRight } from 'lucide-react';
import { cn } from '../../../utils/cn';
import { Button } from '../../../components/ui/Button';
import { Icon } from '../../../components/ui/Icon';
import type { OrchestratorRunEvent, WaveCompletedEvent } from '../../../types/council';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface WaveBoundary {
  /** Zero-based wave index. */
  waveIndex: number;
  /** Number of nodes that completed in this wave. */
  nodeCount: number;
  /** ISO timestamp of the WaveCompleted event. */
  completedAt: string;
}

interface WaveScrubberProps {
  /** All persisted events for the run. */
  events: OrchestratorRunEvent[];
  /** Called when the user confirms a rewind to the given wave index. */
  onRewind: (waveIndex: number) => void;
  /** Disable all interactions (e.g. while a rewind is in flight). */
  disabled?: boolean;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Extract wave boundary metadata from the event log. */
function extractWaveBoundaries(events: OrchestratorRunEvent[]): WaveBoundary[] {
  const boundaries: WaveBoundary[] = [];
  for (const ev of events) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(ev.event_json);
    } catch {
      continue;
    }
    const p = parsed as { type?: string };
    if (p?.type === 'wave_completed') {
      const wc = parsed as WaveCompletedEvent;
      boundaries.push({
        waveIndex: wc.wave_index,
        nodeCount: wc.node_count,
        completedAt: ev.created_at,
      });
    }
  }
  return boundaries.sort((a, b) => a.waveIndex - b.waveIndex);
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString(undefined, {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  } catch {
    return iso;
  }
}

// ─── Component ───────────────────────────────────────────────────────────────

/**
 * WaveScrubber shows each completed wave as a waypoint the user can rewind to.
 *
 * Renders nothing when there are fewer than two wave boundaries (a single
 * completed wave means there is nothing useful to rewind to).
 */
const WaveScrubber: FC<WaveScrubberProps> = ({ events, onRewind, disabled = false }) => {
  const [pendingWave, setPendingWave] = useState<WaveBoundary | null>(null);
  const boundaries = extractWaveBoundaries(events);

  // Need at least 2 waves to make rewind useful.
  if (boundaries.length < 2) return null;

  const handleClick = (boundary: WaveBoundary) => {
    if (disabled) return;
    setPendingWave(boundary);
  };

  const handleConfirm = () => {
    if (!pendingWave) return;
    onRewind(pendingWave.waveIndex);
    setPendingWave(null);
  };

  const handleCancel = () => {
    setPendingWave(null);
  };

  const eventsAfter = pendingWave
    ? events.filter((ev) => ev.wave_index > pendingWave.waveIndex).length
    : 0;

  return (
    <section
      data-testid="wave-scrubber"
      className="flex flex-col gap-sm"
      aria-label="Wave scrubber — rewind execution"
    >
      <p className="text-xs font-semibold text-text-muted uppercase tracking-wide">
        Time-travel rewind
      </p>

      {/* Wave waypoints */}
      <div className="flex items-center gap-xs overflow-x-auto pb-xs scrollbar-thin" role="list">
        {boundaries.map((b, idx) => (
          <div key={b.waveIndex} className="flex items-center gap-xs shrink-0" role="listitem">
            {idx > 0 && (
              <Icon
                icon={ChevronRight}
                size={11}
                className="text-text-muted shrink-0"
                aria-hidden="true"
              />
            )}
            <button
              type="button"
              disabled={disabled}
              title={`Rewind to after wave ${b.waveIndex} (${b.nodeCount} node${b.nodeCount !== 1 ? 's' : ''} — ${formatTime(b.completedAt)})`}
              aria-label={`Rewind to wave ${b.waveIndex}`}
              onClick={() => handleClick(b)}
              className={cn(
                'flex flex-col items-center px-sm py-xs rounded-sm border text-xs transition-colors',
                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary',
                disabled
                  ? 'border-border bg-surface text-text-disabled cursor-not-allowed'
                  : 'border-border bg-surface-elevated text-text-secondary hover:border-primary/50 hover:bg-primary/5 hover:text-primary cursor-pointer',
              )}
            >
              <span className="font-semibold">W{b.waveIndex}</span>
              <span className="text-[10px] text-text-muted">
                {b.nodeCount}n
              </span>
            </button>
          </div>
        ))}
      </div>

      {/* Confirmation dialog */}
      {pendingWave && (
        <div
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="rewind-confirm-title"
          aria-describedby="rewind-confirm-desc"
          className="rounded-base border border-warning/40 bg-warning/8 px-md py-sm flex flex-col gap-sm"
        >
          <p
            id="rewind-confirm-title"
            className="text-sm font-semibold text-text"
          >
            Rewind to wave {pendingWave.waveIndex}?
          </p>
          <p id="rewind-confirm-desc" className="text-xs text-text-secondary leading-relaxed">
            This will discard{' '}
            <strong>{eventsAfter} event{eventsAfter !== 1 ? 's' : ''}</strong>{' '}
            from waves after wave {pendingWave.waveIndex} and re-execute from
            that point. This action cannot be undone.
          </p>
          <div className="flex gap-sm justify-end">
            <Button variant="secondary" size="sm" onClick={handleCancel}>
              Cancel
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={handleConfirm}
              data-testid="rewind-confirm-btn"
            >
              <Icon icon={RotateCcw} size={12} />
              Rewind &amp; re-execute
            </Button>
          </div>
        </div>
      )}
    </section>
  );
};

export default WaveScrubber;
