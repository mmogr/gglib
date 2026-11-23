/**
 * The devices line: the paired machines and phones, on every screen.
 *
 * A `PlaceRow`: the devices icon, `Devices`, and at the right what the
 * tunnel is doing in the daemon's words, its shape as the icon's badge. The
 * machine this one has joined says its reach (`REACH_WORDS`: Connected,
 * Away, Disconnected) with the machine's name under it; else sharing says
 * the daemon's word (`status.sharing`, "Sharing") with its count under it
 * (`status.paired_line`, "2 paired") while this machine lets devices in;
 * else nothing. A pairing code still on offer is said as "code 1:42",
 * counting down to the code's own deadline, since it outlives the page that
 * revealed it (DV-1); it is plain text with no badge, as the pairing sheet's
 * "Expires in 1:42" is, since ▲ is for warnings only. Before the first
 * status the badge turns, as "asking the daemon" does.
 *
 * Clicking the line opens the Devices place.
 *
 * @module sidebar/DevicesLine
 */

import { useNavigate } from 'react-router';
import { MonitorSmartphone } from 'lucide-react';
import { paths } from '../app/paths';
import { REACH_MARK, type MarkKind } from '../components/status/StatusMark';
import { REACH_WORDS } from '../components/status/words';
import { useSecondsLeft } from '../hooks/useSecondsLeft';
import { clock } from '../utils/countdown';
import { pairedName, useRemoteState } from '../services/remoteRegistry';
import type { RemoteStatus } from '../services/transport/types/remote';
import { PlaceRow } from './PlaceRow';

/**
 * What the line says, from the last status and any offer: `mark` is the
 * badge (`null` for plain text), `word` the right edge, `detail` the line
 * under the name (empty for none).
 */
export type DevicesSaying = { mark: MarkKind | null; word: string; detail: string } | null;

/** `codeLeft`: the seconds the code on offer has left, or `null` with none. */
export function devicesSaying(status: RemoteStatus | null, codeLeft: number | null): DevicesSaying {
  if (codeLeft !== null) return { mark: null, word: `code ${clock(codeLeft)}`, detail: status?.paired_line ?? '' };
  if (status === null) return { mark: 'changing', word: '', detail: '' };
  if (status.connected !== null) {
    return { mark: REACH_MARK[status.reach], word: REACH_WORDS[status.reach], detail: pairedName(status) };
  }
  if (status.enabled) return { mark: 'on', word: status.sharing, detail: status.paired_line };
  return null;
}

export function DevicesLine({ current = false }: { current?: boolean }) {
  const { status, offer } = useRemoteState();
  const navigate = useNavigate();
  const codeLeft = useSecondsLeft(offer?.expiresAt ?? null);

  const saying = devicesSaying(status, offer === null ? null : codeLeft);
  const said = saying === null ? '' : [saying.word, saying.detail].filter(Boolean).map((w) => `, ${w}`).join('');

  return (
    <PlaceRow
      name="Devices"
      icon={MonitorSmartphone}
      badge={saying?.mark ? { kind: saying.mark, label: saying.word || 'Asking the daemon' } : null}
      word={saying?.word ? <span className="truncate">{saying.word}</span> : undefined}
      under={saying?.detail ? <span className="truncate">{saying.detail}</span> : undefined}
      current={current}
      onClick={() => navigate(paths.devices())}
      label={`Devices${said}`}
    />
  );
}
