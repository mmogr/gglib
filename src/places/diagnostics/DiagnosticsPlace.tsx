/**
 * The diagnostics place, at `/diagnostics/:tab`: Live, Logs, Benchmarks and
 * System as a tablist, the open tab named by the address. A segment that
 * names no tab goes to Live.
 *
 * Benchmarks reads `?models=` for the models picked to run (a model's
 * ⋯ Benchmark… sends them), and its code loads only when the tab opens.
 * Once opened it stays mounted, hidden while another tab is open, so a
 * run in flight is not aborted by a look at Live or Logs.
 *
 * @module places/diagnostics/DiagnosticsPlace
 */

import { lazy, Suspense, useState, type FC } from 'react';
import { Navigate, useLocation, useNavigate, useParams } from 'react-router';
import { benchmarkModelIds, DIAGNOSTICS_TABS, diagnosticsTab, paths, type DiagnosticsTab } from '../../app/paths';
import { StatusMark } from '../../components/status/StatusMark';
import { Tabs } from '../../components/ui/Tabs';
import { useModelLibrary } from '../../services/modelRegistry';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { LiveTab } from './live/LiveTab';
import { LogsTab } from './LogsTab';

// Lazy: keeps the benchmark code out of the first load.
const BenchmarkPage = lazy(() => import('./benchmarks/BenchmarkPage'));

/** Each tab's name in the tablist. */
export const TAB_NAMES: Readonly<Record<DiagnosticsTab, string>> = {
  live: 'Live',
  logs: 'Logs',
  benchmarks: 'Benchmarks',
  system: 'System',
};

function BenchmarksTab() {
  const { models } = useModelLibrary();
  const ids = benchmarkModelIds(useLocation().search);
  return (
    <Suspense fallback={<StatusMark kind="changing" label="Loading" />}>
      <BenchmarkPage models={models} initialModelIds={ids} />
    </Suspense>
  );
}

function SystemTab() {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto py-md">
      <div className="flex max-w-[720px] flex-col px-md">
        <DiagnosticsPanel />
      </div>
    </div>
  );
}

/** The tabs drawn afresh each time they open; Benchmarks is kept (below). */
const CONTENT: Readonly<Record<Exclude<DiagnosticsTab, 'benchmarks'>, FC>> = {
  live: LiveTab,
  logs: LogsTab,
  system: SystemTab,
};

export default function DiagnosticsPlace() {
  const tab = diagnosticsTab(useParams().tab);
  const navigate = useNavigate();
  const [benchmarksOpened, setBenchmarksOpened] = useState(false);
  if (tab === 'benchmarks' && !benchmarksOpened) setBenchmarksOpened(true);
  if (tab === null) return <Navigate to={paths.diagnostics()} replace />;
  const Content = tab === 'benchmarks' ? null : CONTENT[tab];

  return (
    <section aria-label="Diagnostics" className="flex min-w-0 flex-1 flex-col overflow-hidden">
      <header className="shrink-0 px-md pt-md">
        <h2 className="m-0 text-base font-semibold text-text">Diagnostics</h2>
      </header>
      <Tabs<DiagnosticsTab>
        aria-label="Diagnostics"
        className="shrink-0 px-sm"
        activeId={tab}
        onChange={(id) => navigate(paths.diagnostics(id))}
        tabs={DIAGNOSTICS_TABS.map((id) => ({ id, label: TAB_NAMES[id] }))}
      />
      {Content && (
        <div role="tabpanel" aria-label={TAB_NAMES[tab]} className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <Content key={tab} />
        </div>
      )}
      {benchmarksOpened && (
        <div role="tabpanel" aria-label={TAB_NAMES.benchmarks} hidden={tab !== 'benchmarks'} className="min-h-0 flex-1 flex-col overflow-hidden [&:not([hidden])]:flex">
          <BenchmarksTab />
        </div>
      )}
    </section>
  );
}
