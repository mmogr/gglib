/**
 * Diagnostics › Logs: a loaded model's llama-server output (the Log
 * subject's panel, `inspector/log/ConsoleLogPanel`), picked from the models
 * loaded now, with its facts in a column beside it (`logFacts`). The first
 * loaded model is picked until another is; a pick that unloads falls back
 * to the first again. With nothing loaded, one sentence says when a log
 * shows.
 *
 * @module places/diagnostics/LogsTab
 */

import { useState } from 'react';
import ConsoleLogPanel from '../../inspector/log/ConsoleLogPanel';
import { useServers } from '../../hooks/useServers';
import { Note } from '../library/Group';
import { useEndpointDashboard } from '../library/useResidency';
import { logFacts } from './logFacts';

export function LogsTab() {
  const { servers } = useServers();
  const { snapshot } = useEndpointDashboard();
  const [picked, setPicked] = useState<number | null>(null);
  const loaded = servers.filter((s) => s.status === 'running' && s.port > 0);
  const server = loaded.find((s) => s.modelId === picked) ?? loaded[0];

  if (server === undefined) {
    return (
      <div className="py-md">
        <Note>Nothing is loaded. A model&apos;s log shows here while it is loaded.</Note>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden wide:flex-row">
      <aside aria-label="Facts" className="flex shrink-0 flex-col gap-md overflow-y-auto px-md py-md wide:w-[240px]">
        <label className="flex flex-col gap-xs text-xs font-semibold text-text-secondary">
          Model
          <select
            className="rounded-base bg-surface-elevated px-sm py-xs text-sm font-normal text-text"
            value={server.modelId}
            onChange={(e) => setPicked(Number(e.target.value))}
          >
            {loaded.map((s) => (
              <option key={s.modelId} value={s.modelId}>
                {s.modelName}
              </option>
            ))}
          </select>
        </label>
        <dl className="m-0 flex flex-col gap-xs text-xs tabular-nums">
          {logFacts(server, snapshot).map((f) => (
            <div key={f.label} className="flex justify-between gap-md">
              <dt className="text-text-secondary">{f.label}</dt>
              <dd className="m-0 text-right text-text">{f.value}</dd>
            </div>
          ))}
        </dl>
      </aside>
      <ConsoleLogPanel key={server.port} serverPort={server.port} modelName={server.modelName} />
    </div>
  );
}
