/**
 * `/diagnostics/benchmarks?models=…`: the benchmarks, with the named models
 * picked to run. Close goes back to the first of them, or the library.
 *
 * @module app/BenchmarkRoute
 */

import { lazy, Suspense } from 'react';
import { useLocation, useNavigate } from 'react-router';

import { useModelLibrary } from '../services/modelRegistry';
import { benchmarkModelIds, paths } from './paths';

// Lazy: keeps the benchmark code out of the first load.
const BenchmarkPage = lazy(() => import('../pages/BenchmarkPage'));

export function BenchmarkRoute() {
  const location = useLocation();
  const navigate = useNavigate();
  const { models } = useModelLibrary();
  const ids = benchmarkModelIds(location.search);
  return (
    <Suspense
      fallback={
        <div className="flex flex-col h-full w-full overflow-hidden items-center justify-center text-text-muted">
          Loading benchmark...
        </div>
      }
    >
      <BenchmarkPage
        models={models}
        initialModelIds={ids}
        onClose={() => navigate(ids[0] !== undefined ? paths.model(ids[0]) : paths.library())}
      />
    </Suspense>
  );
}
