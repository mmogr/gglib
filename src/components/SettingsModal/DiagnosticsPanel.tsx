/**
 * Diagnostics — `gglib config check-deps`, `paths` and `fast-downloads
 * status` as one panel in the System tab.
 *
 * This is the "why isn't this working" surface, so it leads with what is
 * wrong: missing required dependencies first, with their install commands,
 * then the resolved paths that answer "which directory is it actually using".
 */

import { FC } from 'react';
import { Button } from '../ui/Button';
import { Banner } from '../ui/Banner';
import { Stack } from '../primitives';
import { useDiagnostics } from './useDiagnostics';
import type { DependencyInfo, ResolvedPaths } from '../../types/setup';

/** Status dot plus name; the dot carries no meaning on its own, so the word stays. */
const DependencyRow: FC<{ dep: DependencyInfo }> = ({ dep }) => {
  const dot =
    dep.status === 'present'
      ? 'bg-success'
      : dep.required
        ? 'bg-danger'
        : 'bg-text-muted';

  return (
    <div className="flex items-baseline justify-between gap-md py-2xs">
      <div className="flex items-baseline gap-xs min-w-0">
        <span
          className={`inline-block w-2 h-2 rounded-full shrink-0 ${dot}`}
          aria-hidden="true"
        />
        <span className="text-xs text-text font-mono">{dep.name}</span>
        {!dep.required && <span className="text-2xs text-text-muted">optional</span>}
      </div>
      <span className="text-xs text-text-secondary font-mono tabular-nums text-right">
        {dep.status === 'present' ? (dep.version ?? 'present') : dep.status}
      </span>
    </div>
  );
};

const PATH_LABELS: [keyof ResolvedPaths, string][] = [
  ['modelsDir', 'Models'],
  ['dataRoot', 'Data'],
  ['databasePath', 'Database'],
  ['llamaServerPath', 'llama-server'],
  ['resourceRoot', 'Resources'],
];

const MODELS_SOURCE_COPY: Record<ResolvedPaths['modelsSource'], string> = {
  explicit: 'set explicitly',
  envVar: 'from the environment',
  default: 'platform default',
};

export const DiagnosticsPanel: FC = () => {
  const {
    diagnostics,
    loading,
    error,
    reload,
    togglingAccelerator,
    acceleratorError,
    toggleAccelerator,
  } = useDiagnostics();

  if (loading && !diagnostics) {
    return <p className="text-sm text-text-muted m-0">Running diagnostics…</p>;
  }

  if (error) {
    return (
      <Banner variant="danger" action={<Button size="sm" variant="secondary" onClick={reload}>Retry</Button>}>
        {error}
      </Banner>
    );
  }

  if (!diagnostics) return null;

  const { dependencies, paths, acceleration, fastDownloads } = diagnostics;
  const missingRequired = dependencies.filter((d) => d.required && d.status !== 'present');
  // Several dependencies share an install command (one apt line covering
  // three packages, say); printing it once per dependency is just noise.
  const installHints = [
    ...new Set(missingRequired.map((d) => d.installHint).filter(Boolean) as string[]),
  ];

  return (
    <Stack gap="xl">
      <section>
        <div className="flex items-center justify-between gap-md mb-xs">
          <h3 className="m-0 text-sm font-semibold text-text">Dependencies</h3>
          <Button variant="ghost" size="sm" isLoading={loading} onClick={reload}>
            Re-check
          </Button>
        </div>

        {missingRequired.length > 0 && (
          <Banner variant="warning" className="mb-base">
            <Stack gap="xs">
              <span>
                {missingRequired.length} required{' '}
                {missingRequired.length === 1 ? 'dependency is' : 'dependencies are'} missing.
                These are the tools needed to build gglib itself from source; a downloaded
                build does not need them, and they are listed here because
                <code className="font-mono"> gglib config check-deps</code> reports them.
              </span>
              {installHints.map((hint) => (
                <code key={hint} className="text-xs font-mono break-all">
                  {hint}
                </code>
              ))}
            </Stack>
          </Banner>
        )}

        <div className="flex flex-col">
          {dependencies.map((dep) => (
            <DependencyRow key={dep.name} dep={dep} />
          ))}
        </div>
      </section>

      <section>
        <h3 className="m-0 mb-xs text-sm font-semibold text-text">Acceleration</h3>
        {acceleration.detected ? (
          <p className="m-0 text-xs text-text-secondary">
            A build here would use{' '}
            <span className="font-mono">{acceleration.detected}</span>.
          </p>
        ) : (
          <Banner variant="warning">
            {acceleration.detectionError ??
              'No supported GPU acceleration was detected on this machine.'}
          </Banner>
        )}
      </section>

      <section>
        <h3 className="m-0 mb-xs text-sm font-semibold text-text">Paths</h3>
        <p className="m-0 mb-base text-xs text-text-muted">
          Where gglib actually reads and writes — the answer when a model or setting seems to
          have gone missing.
        </p>
        <Stack gap="xs">
          {PATH_LABELS.map(([key, label]) => (
            <div key={key} className="flex items-baseline justify-between gap-md">
              <span className="text-xs text-text-muted shrink-0">{label}</span>
              <span className="text-xs text-text-secondary font-mono text-right break-all">
                {paths[key]}
              </span>
            </div>
          ))}
          <p className="m-0 text-2xs text-text-muted text-right">
            Models directory {MODELS_SOURCE_COPY[paths.modelsSource]}.
          </p>
        </Stack>
      </section>

      <section>
        <h3 className="m-0 mb-xs text-sm font-semibold text-text">Download accelerator</h3>
        <p className="m-0 mb-base text-xs text-text-muted">
          The optional hf_xet helper speeds up HuggingFace downloads. Downloads work without it,
          over plain HTTP — this is a speed setting, not a requirement.
        </p>

        {fastDownloads.error && (
          <Banner variant="danger" className="mb-base">
            {fastDownloads.error}
          </Banner>
        )}
        {acceleratorError && (
          <Banner variant="danger" className="mb-base">
            {acceleratorError}
          </Banner>
        )}

        <Stack gap="xs" className="mb-base">
          <div className="flex items-baseline justify-between gap-md">
            <span className="text-xs text-text-muted">Status</span>
            <span className="text-xs text-text-secondary">
              {fastDownloads.provisioned ? 'Enabled' : 'Not enabled'}
            </span>
          </div>
          {fastDownloads.provisioned && fastDownloads.builder && (
            <div className="flex items-baseline justify-between gap-md">
              <span className="text-xs text-text-muted">Built with</span>
              <span className="text-xs text-text-secondary font-mono">{fastDownloads.builder}</span>
            </div>
          )}
          {fastDownloads.envDir && (
            <div className="flex items-baseline justify-between gap-md">
              <span className="text-xs text-text-muted">Environment</span>
              <span className="text-xs text-text-secondary font-mono text-right break-all">
                {fastDownloads.envDir}
              </span>
            </div>
          )}
        </Stack>

        <Button
          variant={fastDownloads.provisioned ? 'dangerGhost' : 'secondary'}
          size="sm"
          isLoading={togglingAccelerator}
          disabled={togglingAccelerator}
          onClick={() => void toggleAccelerator(!fastDownloads.provisioned)}
        >
          {fastDownloads.provisioned ? 'Disable accelerator' : 'Enable accelerator'}
        </Button>
      </section>
    </Stack>
  );
};
