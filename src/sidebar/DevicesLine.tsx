/**
 * The devices line: the paired machines and phones, on every screen.
 *
 * `◎ Devices` and, at the right, what the tunnel is doing in the daemon's
 * words: the machine this one has joined with its reach (`REACH_WORDS`:
 * Connected, Away, Disconnected), else "Sharing · N paired" while this
 * machine lets devices in, else nothing. A pairing code still on offer is
 * said as "code 1:42", counting down to the code's own deadline, since it
 * outlives the page that revealed it (DV-1). Before the first status the
 * mark turns, as "asking the daemon" does.
 *
 * Clicking the line opens the Devices place.
 *
 * @module sidebar/DevicesLine
 */

import { useNavigate } from 'react-router';
import { paths } from '../app/paths';
import { REACH_MARK, StatusMark, type MarkKind } from '../components/status/StatusMark';
import { REACH_WORDS } from '../components/status/words';
import { Button } from '../components/ui/Button';
import { clock } from '../places/devices/devices';
import { useSecondsLeft } from '../places/devices/useSecondsLeft';
import { pairedName, useRemoteState } from '../services/remoteRegistry';
import type { RemoteStatus } from '../services/transport/types/remote';
import { cn } from '../utils/cn';
import { SPREAD } from './placement';

/** What the line says at its right, from the last status and any offer. */
export type DevicesSaying = { mark: MarkKind; word: string; name?: string } | null;

/** `codeLeft`: the seconds the code on offer has left, or `null` with none. */
export function devicesSaying(status: RemoteStatus | null, codeLeft: number | null): DevicesSaying {
  if (codeLeft !== null) return { mark: 'attention', word: `code ${clock(codeLeft)}` };
  if (status === null) return { mark: 'changing', word: '' };
  if (status.connected !== null) {
    return { mark: REACH_MARK[status.reach], word: REACH_WORDS[status.reach], name: pairedName(status) };
  }
  if (status.enabled) {
    // `?? []`: a daemon from before the roster answers status without one.
    const paired = (status.devices ?? []).length;
    return { mark: 'on', word: `Sharing · ${paired} paired` };
  }
  return null;
}

export function DevicesLine({ current = false }: { current?: boolean }) {
  const { status, offer } = useRemoteState();
  const navigate = useNavigate();
  const codeLeft = useSecondsLeft(offer?.expiresAt ?? null);

  const saying = devicesSaying(status, offer === null ? null : codeLeft);
  const said = saying === null ? '' : `, ${[saying.name, saying.word].filter(Boolean).join(' ')}`;

  return (
    <div className="px-sm">
      <Button
        variant="ghost"
        fullWidth
        className={cn('h-auto gap-sm px-sm py-sm', SPREAD, current && 'bg-surface-hover')}
        onClick={() => navigate(paths.devices())}
        aria-current={current ? 'page' : undefined}
        aria-label={`Devices${said}`}
      >
        <span className="text-sm text-text">Devices</span>
        {saying !== null && (
          <span className="flex items-center gap-xs text-sm text-text-secondary truncate">
            {saying.name !== undefined && <span className="truncate">{saying.name}</span>}
            <StatusMark kind={saying.mark} word={saying.word || undefined} label={saying.word || 'Asking the daemon'} />
          </span>
        )}
      </Button>
    </div>
  );
}
