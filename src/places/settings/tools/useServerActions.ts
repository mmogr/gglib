/**
 * What a tool server row's actions do, over the section's one list (ST-8):
 * Test, Start, Stop, Remove… (asked first) and Fix, with the server they are
 * busy on and how the last one went, each kept to its own row.
 *
 * Test is `gglib mcp test`: a throwaway instance started, its tools listed,
 * then stopped, so it leaves nothing running. One test at a time across the
 * list: two would each start an instance, and a failure would land on
 * whichever row was clicked second. Each row is busy on its own, so one
 * action ending never frees another row's buttons early. Fix is the daemon's path resolution;
 * once it resolves, the list is read again so the row stops needing it.
 *
 * @module places/settings/tools/useServerActions
 */

import { useCallback, useState } from 'react';
import { useConfirmContext } from '../../../contexts/ConfirmContext';
import type { UseMcpServersResult } from '../../../hooks/useMcpServers';
import { getTransport, type McpServerId, type McpServerInfo, type McpTestResult } from '../../../services/transport';
import { formatError } from '../../../utils/errors';

/** How one row's last action went, said under it. */
export type RowOutcome =
  | { kind: 'tested'; result: McpTestResult }
  | { kind: 'fixed'; path: string }
  | { kind: 'refused'; message: string };

export interface ServerActions {
  /** The servers an action is on its way for. */
  busy: ReadonlySet<McpServerId>;
  /** Whether a test is running, on any row. */
  testing: boolean;
  outcome: (id: McpServerId) => RowOutcome | null;
  test: (info: McpServerInfo) => Promise<void>;
  start: (info: McpServerInfo) => Promise<void>;
  stop: (info: McpServerInfo) => Promise<void>;
  remove: (info: McpServerInfo) => Promise<void>;
  fix: (info: McpServerInfo) => Promise<void>;
}

/** The sentence for a path the daemon could not resolve. */
function unresolved(status: Awaited<ReturnType<ReturnType<typeof getTransport>['resolveMcpServerPath']>>): string {
  const allNotFound = status.attempts.every((a) => a.outcome.toLowerCase().includes('not found'));
  const parts = ['Could not resolve executable.'];
  if (status.suggested_fix) parts.push(`Suggested fix: ${status.suggested_fix}`);
  if (allNotFound) parts.push('Install Node.js if not installed (includes npm/npx).');
  return parts.join(' ');
}

export function useServerActions(mcp: UseMcpServersResult): ServerActions {
  const { startServer, stopServer, removeServer, refresh } = mcp;
  const { confirm } = useConfirmContext();
  const [busy, setBusy] = useState<ReadonlySet<McpServerId>>(new Set());
  const [testing, setTesting] = useState(false);
  const [outcomes, setOutcomes] = useState<ReadonlyMap<McpServerId, RowOutcome>>(new Map());

  const said = useCallback((id: McpServerId, outcome: RowOutcome | null) => {
    setOutcomes((prev) => {
      const next = new Map(prev);
      if (outcome === null) next.delete(id);
      else next.set(id, outcome);
      return next;
    });
  }, []);

  /** Run `work` for `info`'s row, busy while it runs, a throw said under the row. */
  const run = useCallback(
    async (info: McpServerInfo, work: () => Promise<RowOutcome | null>) => {
      const id = info.server.id;
      said(id, null);
      setBusy((prev) => new Set(prev).add(id));
      try {
        said(id, await work());
      } catch (e) {
        said(id, { kind: 'refused', message: formatError(e) });
      } finally {
        setBusy((prev) => {
          const next = new Set(prev);
          next.delete(id);
          return next;
        });
      }
    },
    [said],
  );

  const test = useCallback(
    async (info: McpServerInfo) => {
      setTesting(true);
      try {
        await run(info, async () => ({ kind: 'tested', result: await getTransport().testMcpServer(info.server.id) }));
      } finally {
        setTesting(false);
      }
    },
    [run],
  );

  const start = useCallback(
    (info: McpServerInfo) =>
      run(info, async () => {
        await startServer(info.server.id);
        return null;
      }),
    [run, startServer],
  );

  const stop = useCallback(
    (info: McpServerInfo) =>
      run(info, async () => {
        await stopServer(info.server.id);
        return null;
      }),
    [run, stopServer],
  );

  const remove = useCallback(
    async (info: McpServerInfo) => {
      const yes = await confirm({
        title: `Remove "${info.server.name}"?`,
        description: 'This cannot be undone.',
        confirmLabel: 'Remove',
        variant: 'danger',
      });
      if (!yes) return;
      await run(info, async () => {
        await removeServer(info.server.id);
        return null;
      });
    },
    [confirm, run, removeServer],
  );

  const fix = useCallback(
    (info: McpServerInfo) =>
      run(info, async () => {
        const status = await getTransport().resolveMcpServerPath(info.server.id);
        if (!status.success) return { kind: 'refused', message: unresolved(status) };
        await refresh();
        return { kind: 'fixed', path: status.resolved_path ?? '' };
      }),
    [run, refresh],
  );

  const outcome = useCallback((id: McpServerId) => outcomes.get(id) ?? null, [outcomes]);

  return { busy, testing, outcome, test, start, stop, remove, fix };
}
