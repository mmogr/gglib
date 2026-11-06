/**
 * The devices line: the paired machines and phones, on every screen.
 *
 * `◎ Devices` and, at the right, what the tunnel is doing in the daemon's
 * words: the machine this one has joined with its reach (`REACH_WORDS`:
 * Connected, Away, Disconnected), else "Sharing · N paired" while this
 * machine lets devices in, else nothing. A pairing code still on offer is
 * said too, since it outlives the panel that revealed it (DV-1). Before the
 * first status the mark turns, as "asking the daemon" does.
 *
 * Clicking the line opens today's remote panel beside it, until the Devices
 * place takes the click (V1).
 *
 * @module sidebar/DevicesLine
 */

import { useRef, useState } from 'react';
import { RemotePopover } from '../components/RemoteControl';
import { StatusMark, type MarkKind } from '../components/status/StatusMark';
import { REACH_WORDS } from '../components/status/words';
import { Button } from '../components/ui/Button';
import { useClickOutside } from '../hooks/useClickOutside';
import { pairedName, useRemoteState } from '../services/remoteRegistry';
import type { RemoteStatus } from '../services/transport/types/remote';
import type { ReachState } from '../types/generated/ReachState';
import { cn } from '../utils/cn';
import { POPOVER_BESIDE, SPREAD } from './placement';

const REACH_MARK: Record<ReachState, MarkKind> = {
  connected: 'on',
  away: 'away',
  disconnected: 'off',
};

/** What the line says at its right, from the last status and any offer. */
export type DevicesSaying = { mark: MarkKind; word: string; name?: string } | null;

export function devicesSaying(status: RemoteStatus | null, codeWaiting: boolean): DevicesSaying {
  if (codeWaiting) return { mark: 'attention', word: 'Code waiting' };
  if (status === null) return { mark: 'changing', word: '' };
  if (status.connected !== null) {
    return { mark: REACH_MARK[status.reach], word: REACH_WORDS[status.reach], name: pairedName(status) };
  }
  if (status.enabled) {
    const paired = status.devices.length;
    return { mark: 'on', word: `Sharing · ${paired} paired` };
  }
  return null;
}

export function DevicesLine() {
  const { status, offer } = useRemoteState();
  const [open, setOpen] = useState(false);
  const lineRef = useRef<HTMLDivElement>(null);

  useClickOutside(lineRef, () => setOpen(false), open);

  const saying = devicesSaying(status, offer !== null);
  const said = saying === null ? '' : `, ${[saying.name, saying.word].filter(Boolean).join(' ')}`;

  return (
    <div className="relative px-sm" ref={lineRef}>
      <Button
        variant="ghost"
        fullWidth
        className={cn('h-auto gap-sm px-sm py-sm', SPREAD)}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-label={`Devices${said}`}
        // A code outlives the panel, so the closed line says one is waiting:
        // someone who clicked away mid-pairing knows to open it again.
        title={offer !== null ? 'A pairing code is waiting' : undefined}
      >
        <span className="text-sm text-text">Devices</span>
        {saying !== null && (
          <span className="flex items-center gap-xs text-sm text-text-secondary truncate">
            {saying.name !== undefined && <span className="truncate">{saying.name}</span>}
            <StatusMark kind={saying.mark} word={saying.word || undefined} label={saying.word || 'Asking the daemon'} />
          </span>
        )}
      </Button>

      <RemotePopover open={open} className={POPOVER_BESIDE} />
    </div>
  );
}
