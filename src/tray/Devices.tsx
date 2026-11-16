/**
 * The tray's Devices block: the machine this one has joined, with its reach
 * (`REACH_WORDS`: Connected, Away, Disconnected), only while one is; then
 * this machine in the service's words (`useThisMachine`: This Mac, This PC),
 * sharing in the daemon's words (`sharingSentence`: "Sharing ·
 * N paired", "Switched on, nothing bound", "Not sharing"), which is a fact
 * whatever it says. Not drawn before the first status.
 *
 * The facts are `remoteRegistry`'s; the joined machine's name is
 * `pairedName`'s.
 *
 * @module tray/Devices
 */

import type { FC } from 'react';
import { REACH_MARK, StatusMark, type MarkKind } from '../components/status/StatusMark';
import { REACH_WORDS } from '../components/status/words';
import { Group } from '../places/library/Group';
import { useThisMachine, type MachineWords } from '../services/hostRegistry';
import { pairedName, sharingSentence } from '../services/remoteRegistry';
import type { RemoteStatus } from '../services/transport/types/remote';

/** One line of the block: what it is about, and its state. */
export interface DeviceLine {
  label: string;
  mark: MarkKind;
  word: string;
}

/** The block's lines: the joined machine while there is one, then this one's sharing; none before a status. */
export function devicesLines(status: RemoteStatus | null, machine: MachineWords): DeviceLine[] {
  if (status === null) return [];
  const lines: DeviceLine[] = [];
  if (status.connected !== null) {
    lines.push({ label: pairedName(status), mark: REACH_MARK[status.reach], word: REACH_WORDS[status.reach] });
  }
  lines.push({ label: machine.title, mark: status.enabled ? 'on' : 'off', word: sharingSentence(status) });
  return lines;
}

export const Devices: FC<{ status: RemoteStatus | null }> = ({ status }) => {
  const lines = devicesLines(status, useThisMachine());
  if (lines.length === 0) return null;
  return (
    <Group label="Devices">
      <ul className="m-0 flex list-none flex-col p-0">
        {lines.map((line, i) => (
          // Two lines at most, always in this order.
          <li key={i} className="flex min-h-[36px] items-center gap-sm pl-lg pr-md">
            <span className="min-w-0 flex-1 truncate text-sm text-text">{line.label}</span>
            <StatusMark kind={line.mark} word={line.word || undefined} label="Asking the daemon" className="shrink-0" />
          </li>
        ))}
      </ul>
    </Group>
  );
};
