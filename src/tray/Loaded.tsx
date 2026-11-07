/**
 * The tray's Loaded block: one row per model in memory, its residency word
 * and × to unload it.
 *
 * The rows are the dashboard's resident slots (`places/endpoint/lines`
 * `loadedRows`, the endpoint place's), whose words move with each request:
 * Working, Loaded, "Reading 61%". While the dashboard is idle (the endpoint
 * off, or its key not yet revealed) they are the server list's running
 * servers, each Loaded, since a model stays in memory with the endpoint
 * off; those only once the list's read has landed. "Nothing loaded" is said
 * only once one of the two has spoken: the dashboard's frame, or the server
 * list while the endpoint is off. While off it is the bare "Nothing
 * loaded.": no request can arrive to load one.
 *
 * × asks the daemon whatever this window believes (TX-12): the row may be
 * the dashboard's while the server list has not heard of it yet. A failure
 * is said under the block.
 *
 * @module tray/Loaded
 */

import { useState, type FC } from 'react';
import { X } from 'lucide-react';
import { RESIDENCY_MARK, StatusMark } from '../components/status/StatusMark';
import { RESIDENCY_WORDS } from '../components/status/words';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';
import type { ServerViewModel } from '../hooks/useServers';
import { Group, Note } from '../places/library/Group';
import { loadedRows } from '../places/endpoint/lines';
import { safeStopServer } from '../services/server/safeActions';
import type { DashboardSnapshot } from '../services/transport/types/dashboard';
import type { ResidencyState } from '../types/generated/ResidencyState';
import { formatError } from '../utils/errors';
import { Failure } from './Failure';

/** One loaded model as the tray says it. */
export interface TrayLoadedRow {
  modelId: number;
  name: string;
  state: ResidencyState;
  word: string;
}

/** The rows: the dashboard's while it has spoken, else the running servers, each Loaded. */
export function trayLoadedRows(snapshot: DashboardSnapshot | null, servers: ServerViewModel[]): TrayLoadedRow[] {
  if (snapshot) return loadedRows(snapshot).map(({ modelId, name, state, word }) => ({ modelId, name, state, word }));
  return servers.map((s) => ({ modelId: s.modelId, name: s.modelName, state: 'loaded', word: RESIDENCY_WORDS.loaded }));
}

interface LoadedProps {
  rows: TrayLoadedRow[];
  /** Whether an empty list is known to be empty, not merely not heard yet. */
  known: boolean;
  /** Whether the endpoint is off, so no request will load a model. */
  off: boolean;
}

export const Loaded: FC<LoadedProps> = ({ rows, known, off }) => {
  const [error, setError] = useState<string | null>(null);
  if (rows.length === 0 && !known) return null;

  const unload = (modelId: number) => {
    setError(null);
    void safeStopServer(modelId).catch((err: unknown) => setError(formatError(err)));
  };

  return (
    <Group label="Loaded">
      {rows.length === 0 ? (
        <Note>{off ? 'Nothing loaded.' : 'Nothing loaded. A model loads on its first request.'}</Note>
      ) : (
        <ul aria-label="Loaded models" className="m-0 flex list-none flex-col p-0">
          {rows.map((row) => (
            <li key={row.modelId} className="flex min-h-[36px] items-center gap-sm pl-lg pr-sm">
              <span className="min-w-0 flex-1 truncate text-sm text-text">{row.name}</span>
              <StatusMark kind={RESIDENCY_MARK[row.state]} word={row.word} className="shrink-0" />
              <Button variant="ghost" size="sm" iconOnly aria-label={`Unload ${row.name}`} title={`Unload ${row.name}`} onClick={() => unload(row.modelId)}>
                <Icon icon={X} size={14} />
              </Button>
            </li>
          ))}
        </ul>
      )}
      {error !== null && <Failure message={error} />}
    </Group>
  );
};
