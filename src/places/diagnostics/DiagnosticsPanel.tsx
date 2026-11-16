/**
 * Diagnostics — `gglib config check-deps`, `paths` and `fast-downloads
 * status` as one panel in the System tab.
 *
 * This is the "why isn't this working" surface, so it leads with what is
 * wrong: what this gglib needs and does not have, with the install commands,
 * then the report's groups, then the resolved paths that answer "which
 * directory is it actually using". Every heading, sentence and state word in
 * the dependency section is the report's own, as `check-deps` prints it.
 */

import { FC } from 'react';
import { Button } from '../../components/ui/Button';
import { Banner } from '../../components/ui/Banner';
import { Stack } from '../../components/primitives';
import { FAST_DOWNLOADS_WORDS } from '../../components/status/words';
import { useDiagnostics } from './useDiagnostics';
import type { DependencyGroup, DependencyInfo, ResolvedPaths } from '../../types/setup';

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
        <span className="text-xs text-text font-mono shrink-0">{dep.name}</span>
        <span className="text-xs text-text-muted truncate">{dep.description}</span>
      </div>
      <span className="text-xs text-text-secondary font-mono tabular-nums text-right shrink-0">
        {dep.statusWord}
      </span>
    </div>
  );
};

/** One purpose's heading, what it says about this gglib, and its rows. */
const DependencyGroupSection: FC<{ group: DependencyGroup }> = ({ group }) => (
  <section aria-label={group.title}>
    <h4 className="m-0 text-xs font-semibold text-text">{group.title}</h4>
    <p className="m-0 mb-2xs text-xs text-text-muted">{group.explanation}</p>
    <div className="flex flex-col">
      {group.dependencies.map((dep) => (
        <DependencyRow key={dep.name} dep={dep} />
      ))}
    </div>
  </section>
);

const PATH_LABELS: [keyof ResolvedPaths, string][] = [
  ['modelsDir', 'Models'],
  ['dataRoot', 'Data'],
  ['databasePath', 'Database'],
  ['llamaServerPath', 'llama-server'],
  ['sdServerPath', 'sd-server'],
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

  const { dependencies: report, paths, acceleration, fastDownloads } = diagnostics;
  const missingRequired = report.groups
    .flatMap((group) => group.dependencies)
    .filter((d) => d.required && d.status === 'missing');
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

        {!report.complete && (
          <Banner variant="warning" className="mb-base">
            <Stack gap="xs">
              <span>{report.verdict}</span>
              {installHints.map((hint) => (
                <code key={hint} className="text-xs font-mono break-all">
                  {hint}
                </code>
              ))}
            </Stack>
          </Banner>
        )}

        <Stack gap="base">
          {report.groups.map((group) => (
            <DependencyGroupSection key={group.title} group={group} />
          ))}
        </Stack>
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
          <p className="m-0 text-xs text-text-muted text-right">
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
              {FAST_DOWNLOADS_WORDS[fastDownloads.state]}
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
