/**
 * System settings panel — the GUI face of `gglib config llama`, and of
 * `gglib config sd` in its image runtime section (`ImageRuntimeSettings`).
 *
 * Answers three questions in the order a user asks them: what is installed,
 * where it stands against its pinned release, and what can I do about it.
 * Self-contained like `InferenceProfiles`; all state lives in
 * `useSystemSettings`.
 */

import { FC, useState } from 'react';
import { Download } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Banner } from '../ui/Banner';
import { Stack } from '../primitives';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { getTransport } from '../../services/transport';
import { useSystemSettings } from './useSystemSettings';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { BuildInfo } from './BuildInfo';
import { ImageRuntimeSettings } from './ImageRuntimeSettings';
import { InstallProgress } from '../SetupWizard/InstallProgress';
import type { LlamaStatus } from '../../types/setup';
import { LabelledValue } from './LabelledValue';
import { pinStanding } from './pinStanding';

/** What a source build is updated with, from a terminal. */
const UPDATE_COMMAND = 'gglib config llama update';

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
    installing,
    progress,
    installError,
    installedVersion,
    runInstall,
    uninstalling,
    uninstallResult,
    runUninstall,
  } = useSystemSettings();
  const { confirm } = useConfirmContext();
  const [shuttingDown, setShuttingDown] = useState(false);
  const [shutdownRequested, setShutdownRequested] = useState(false);

  /**
   * `gglib daemon stop`. Lives here rather than in the tray panel, which
   * deliberately keeps window-level actions on the native menu — and unlike
   * that menu's Quit, this reaches the daemon from the web UI too, where
   * there is otherwise no way to stop it.
   *
   * A failed request is not an error: the daemon is shutting down as it
   * replies, so losing the response is the expected outcome.
   */
  const handleShutdown = async () => {
    const confirmed = await confirm({
      title: 'Stop the daemon?',
      description:
        'Stops every running model and shuts down the backend this interface talks to. ' +
        'Nothing on disk is affected, but you will need to start gglib again to use it.',
      confirmLabel: 'Stop',
      variant: 'danger',
    });
    if (!confirmed) return;

    setShuttingDown(true);
    try {
      await getTransport().shutdownDaemon();
    } catch {
      // Expected — the connection dies with the daemon.
    } finally {
      setShuttingDown(false);
      setShutdownRequested(true);
    }
  };

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

  const standing = status ? pinStanding(status.pin, UPDATE_COMMAND) : null;

  const handleUpdate = async () => {
    const confirmed = await confirm({
      title: `Update llama.cpp to ${standing?.update ?? 'the pinned release'}?`,
      description: 'Downloads the pinned release for this machine and replaces the current binaries.',
      confirmLabel: 'Update',
    });
    if (!confirmed) return;
    runInstall();
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
            <LabelledValue label="Binary" value={status.binaryPath} />
            {status.build ? (
              <>
                <LabelledValue label="Built from" value={status.build.version} />
                <LabelledValue label="Acceleration" value={status.build.acceleration} mono={false} />
                <LabelledValue label="Built" value={status.build.buildDate.split('T')[0]} />
              </>
            ) : (
              <LabelledValue
                label="Build record"
                value={
                  status.buildError ??
                  'none — a prebuilt binary, or one installed outside gglib'
                }
                mono={false}
              />
            )}
            {status.runtime && (
              <LabelledValue
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

      {status?.installed && standing?.text && (
        <section>
          <h3 className="m-0 mb-xs text-sm font-semibold text-text">Pinned release</h3>
          <p className="m-0 mb-base text-xs text-text-secondary">{standing.text}</p>

          {standing.fromTerminal && (
            <Banner variant="info" className="mb-base">
              {standing.fromTerminal}
            </Banner>
          )}
          {installError && (
            <Banner variant="danger" className="mb-base">
              {installError}
            </Banner>
          )}
          {installedVersion && (
            <Banner variant="success" className="mb-base">
              llama.cpp {installedVersion} is installed.
            </Banner>
          )}
          {installing && (
            <div className="mb-base">
              <InstallProgress progress={progress} />
            </div>
          )}

          {standing.update && (
            <Button
              variant="secondary"
              size="sm"
              isLoading={installing}
              disabled={installing || uninstalling || !!status.serving}
              title={status.serving ? `Stop ${status.serving} first.` : undefined}
              leftIcon={<Icon icon={Download} size={14} />}
              onClick={() => void handleUpdate()}
            >
              Update to {standing.update}
            </Button>
          )}
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
          disabled={uninstalling || installing || !status?.installed || !!status?.serving}
          title={status?.serving ? `Stop ${status.serving} first.` : undefined}
          onClick={() => void handleUninstall()}
        >
          Uninstall llama.cpp
        </Button>
      </section>

      <ImageRuntimeSettings />

      <DiagnosticsPanel />

      <BuildInfo />

      <section>
        <h3 className="m-0 mb-xs text-sm font-semibold text-text">Daemon</h3>
        <p className="m-0 mb-base text-xs text-text-muted">
          Everything here runs against the gglib daemon. Stopping it stops every running model
          with it, and this interface loses its backend until the daemon is started again — from
          a terminal with <code className="font-mono">gglib web</code>, which starts it and opens
          this interface in your browser, or by reopening the desktop app.
        </p>
        {shutdownRequested && (
          <Banner variant="info" className="mb-base">
            Shutdown requested. This interface will stop responding.
          </Banner>
        )}
        <Button
          variant="dangerGhost"
          size="sm"
          isLoading={shuttingDown}
          disabled={shuttingDown || shutdownRequested}
          onClick={() => void handleShutdown()}
        >
          Stop the daemon
        </Button>
      </section>
    </Stack>
  );
};
