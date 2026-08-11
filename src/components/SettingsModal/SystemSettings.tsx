/**
 * System settings panel — the GUI face of `gglib config llama`.
 *
 * Answers three questions in the order a user asks them: what is installed,
 * has upstream moved, and what can I do about it. Self-contained like
 * `InferenceProfiles`; all state lives in `useSystemSettings`.
 */

import { FC } from 'react';
import { Button } from '../ui/Button';
import { Banner } from '../ui/Banner';
import { Stack } from '../primitives';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { useSystemSettings } from './useSystemSettings';
import type { LlamaStatus } from '../../types/setup';

/** One label/value row. Values are mono so paths and hashes line up. */
const Row: FC<{ label: string; value: string; mono?: boolean }> = ({
  label,
  value,
  mono = true,
}) => (
  <div className="flex items-baseline justify-between gap-md">
    <span className="text-xs text-text-muted shrink-0">{label}</span>
    <span
      className={`text-xs text-text-secondary text-right break-all ${mono ? 'font-mono tabular-nums' : ''}`}
    >
      {value}
    </span>
  </div>
);

/** Installed / degraded / absent, as a dot and a word. */
const InstallState: FC<{ status: LlamaStatus }> = ({ status }) => {
  const [dot, label] = !status.installed
    ? ['bg-text-muted', 'Not installed']
    : status.healthy
      ? ['bg-success', 'Installed']
      : ['bg-danger', 'Installed, not working'];

  return (
    <div className="flex items-center gap-xs">
      <span className={`inline-block w-2 h-2 rounded-full ${dot}`} aria-hidden="true" />
      <span className="text-sm font-semibold text-text">{label}</span>
    </div>
  );
};

export const SystemSettings: FC = () => {
  const {
    status,
    statusError,
    loadingStatus,
    updateCheck,
    checkingUpdates,
    checkError,
    runUpdateCheck,
    updating,
    updateProgress,
    updateError,
    updateResult,
    runUpdate,
    uninstalling,
    uninstallResult,
    runUninstall,
  } = useSystemSettings();
  const { confirm } = useConfirmContext();

  const handleUninstall = async () => {
    const confirmed = await confirm({
      title: 'Uninstall llama.cpp?',
      description:
        'Removes the source checkout, the build configuration, and everything in the binary ' +
        'directory — llama-server and llama-bench both. Your models are not touched, but no ' +
        'model can be served until llama.cpp is installed again.',
      confirmLabel: 'Uninstall',
      variant: 'danger',
    });
    if (!confirmed) return;
    await runUninstall();
  };

  const handleUpdate = async () => {
    const confirmed = await confirm({
      title: 'Update llama.cpp?',
      description:
        'Pulls the latest upstream changes and rebuilds from source with the same acceleration ' +
        'backend. Compiling takes several minutes and replaces the current binary. Running ' +
        'servers keep using the old binary until they restart.',
      confirmLabel: 'Update',
    });
    if (!confirmed) return;
    runUpdate();
  };

  if (loadingStatus && !status) {
    return <p className="text-sm text-text-muted m-0">Reading llama.cpp status…</p>;
  }

  return (
    <Stack gap="xl">
      {statusError && <Banner variant="danger">{statusError}</Banner>}

      <section>
        <div className="flex items-center justify-between gap-md mb-base">
          <h3 className="m-0 text-sm font-semibold text-text">llama.cpp</h3>
          {status && <InstallState status={status} />}
        </div>

        {status?.healthError && (
          <Banner variant="danger" className="mb-base">
            {status.healthError}
          </Banner>
        )}

        {status && (
          <Stack gap="xs">
            <Row label="Binary" value={status.binaryPath} />
            {status.build ? (
              <>
                <Row label="Built from" value={status.build.version} />
                <Row label="Acceleration" value={status.build.acceleration} mono={false} />
                <Row label="Built" value={status.build.buildDate.split('T')[0]} />
              </>
            ) : (
              <Row
                label="Build record"
                value={
                  status.buildError ??
                  'none — a prebuilt binary, or one installed outside gglib'
                }
                mono={false}
              />
            )}
            {status.runtime && (
              <Row
                label="Binary reports"
                value={
                  status.runtime.build
                    ? `b${status.runtime.build}`
                    : 'unidentified build — gglib applies every compensation'
                }
                mono={!!status.runtime.build}
              />
            )}
          </Stack>
        )}
      </section>

      {status?.installed && (
        <section>
          <h3 className="m-0 mb-xs text-sm font-semibold text-text">Updates</h3>
          <p className="m-0 mb-base text-xs text-text-muted">
            Compares the local source checkout against upstream. Contacts GitHub, so it only runs
            when you ask.
          </p>

          {checkError && (
            <Banner variant="danger" className="mb-base">
              {checkError}
            </Banner>
          )}

          {updateCheck && !updateCheck.comparable ? (
            <Banner variant="info" className="mb-base">
              {updateCheck.repoPresent
                ? 'There is a source checkout but no build record, so there is nothing to compare against. Rebuilding from source will record one.'
                : 'No source checkout, so there is nothing to compare — this is a prebuilt install. Rebuilding from source in the setup wizard makes updates available.'}
            </Banner>
          ) : updateCheck ? (
            <Stack gap="xs" className="mb-base">
              <p className="m-0 text-xs text-text-secondary">
                {updateCheck.commitsBehind === 0
                  ? 'Up to date with upstream.'
                  : `${updateCheck.commitsBehind} commit(s) behind upstream.`}
              </p>
              {updateCheck.recentCommits.length > 0 && (
                <ul className="m-0 pl-md list-disc">
                  {updateCheck.recentCommits.map((line) => (
                    <li key={line} className="text-xs text-text-muted font-mono break-all">
                      {line}
                    </li>
                  ))}
                </ul>
              )}
            </Stack>
          ) : null}

          {updateError && (
            <Banner variant="danger" className="mb-base">
              {updateError}
            </Banner>
          )}
          {updateResult && (
            <Banner variant="success" className="mb-base">
              {updateResult}
            </Banner>
          )}
          {updating && (
            <p className="m-0 mb-base text-xs text-text-secondary" role="status">
              {updateProgress ?? 'Building…'}
            </p>
          )}

          <div className="flex gap-sm">
            <Button
              variant="secondary"
              size="sm"
              isLoading={checkingUpdates}
              disabled={checkingUpdates || updating}
              onClick={runUpdateCheck}
            >
              Check for updates
            </Button>
            <Button
              variant="secondary"
              size="sm"
              isLoading={updating}
              disabled={updating || updateCheck?.repoPresent === false}
              onClick={() => void handleUpdate()}
            >
              Update and rebuild
            </Button>
          </div>
        </section>
      )}

      <section>
        <h3 className="m-0 mb-xs text-sm font-semibold text-text">Uninstall</h3>
        <p className="m-0 mb-base text-xs text-text-muted">
          Removes llama.cpp and its binaries. Models stay on disk, but nothing can be served
          until it is installed again.
        </p>
        {uninstallResult && (
          <Banner variant="info" className="mb-base">
            {uninstallResult}
          </Banner>
        )}
        <Button
          variant="dangerGhost"
          size="sm"
          isLoading={uninstalling}
          disabled={uninstalling || updating || !status?.installed}
          onClick={() => void handleUninstall()}
        >
          Uninstall llama.cpp
        </Button>
      </section>
    </Stack>
  );
};
