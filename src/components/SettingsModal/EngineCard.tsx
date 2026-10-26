/**
 * One engine's card in System settings, the same for llama.cpp and for
 * stable-diffusion.cpp: the web face of
 * `gglib config llama|sd install|status|uninstall`.
 *
 * It answers three questions in the order a user asks them: what is
 * installed, where it stands against its pinned release, and what can be
 * done about it. It installs the pinned download with the setup wizard's
 * progress, also over an older one, warns when that download runs on the
 * CPU alone, and removes the engine, never from under a running model.
 * Where the install or the update would be a source build it names the
 * command that runs one. All state lives in `useEngine`.
 */

import type { FC } from 'react';
import { Download, Trash2 } from 'lucide-react';
import { Button } from '../ui/Button';
import { Banner } from '../ui/Banner';
import { Icon } from '../ui/Icon';
import { Stack } from '../primitives';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { InstallProgress } from '../SetupWizard/InstallProgress';
import { ENGINE_PRODUCT } from '../../types/setup';
import type { EngineStatus, RuntimeKind } from '../../types/setup';
import { LabelledValue } from './LabelledValue';
import { pinStanding } from './pinStanding';
import { useEngine } from './useEngine';

/** What each engine's card says about it that the status does not. */
const WORDS: Record<RuntimeKind, { server: string; serves: string; removes: string }> = {
  llama: {
    server: 'llama-server',
    serves: 'chat models',
    removes: 'its binaries, its install record and any source checkout',
  },
  stable_diffusion: {
    server: 'sd-server',
    serves: 'image models',
    removes: 'sd-server, its library and its install record',
  },
};

/** Installed / not working / absent, as a dot and a word. */
const InstallState: FC<{ status: EngineStatus }> = ({ status }) => {
  const [dot, label] = !status.installed
    ? ['bg-text-muted', 'Not installed']
    : status.broken
      ? ['bg-danger', 'Installed, not working']
      : ['bg-success', 'Installed'];
  return (
    <div className="flex items-center gap-xs">
      <span className={`inline-block w-2 h-2 rounded-full ${dot}`} aria-hidden="true" />
      <span className="text-sm font-semibold text-text">{label}</span>
    </div>
  );
};

/** How the install got there, as its record says, or why there is no record to say. */
const Recorded: FC<{ status: EngineStatus }> = ({ status }) => {
  if (!status.install) {
    return (
      <LabelledValue
        label="Install record"
        value={status.recordError ?? 'none — installed outside gglib'}
        mono={false}
      />
    );
  }
  const { built, release, platform, date } = status.install;
  return (
    <>
      <LabelledValue label={built ? 'Built from' : 'Release'} value={release} />
      <LabelledValue label={built ? 'Acceleration' : 'Download'} value={platform} mono={false} />
      <LabelledValue label={built ? 'Built' : 'Downloaded'} value={date.split('T')[0]} />
    </>
  );
};

export const EngineCard: FC<{ kind: RuntimeKind }> = ({ kind }) => {
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
    uninstallError,
    runUninstall,
  } = useEngine(kind);
  const { confirm } = useConfirmContext();
  const product = ENGINE_PRODUCT[kind];
  const words = WORDS[kind];
  const heading = `engine-${kind}-heading`;

  const standing = status ? pinStanding(status.pin, status.updateCommand) : null;
  const serving = status?.serving ?? null;
  const stopFirst = serving ? `Stop ${serving} first.` : undefined;
  const busy = installing || uninstalling || serving !== null;
  // Whether the install is a download is the daemon's to say. Where a
  // download fits and still the install is not one, a source build was here
  // before, and building it again is the command line's.
  const missing = !!status && !status.installed;
  const offersInstall = missing && !!status?.pin.updateDownloads;
  const builtBefore = missing && status?.download !== null && !status?.pin.updateDownloads;
  const offersUpdate = !!status?.installed && !!standing?.update;

  const handleUpdate = async () => {
    const confirmed = await confirm({
      title: `Update ${product} to ${standing?.update ?? 'the pinned release'}?`,
      description: 'Downloads the pinned release for this machine and replaces the one installed.',
      confirmLabel: 'Update',
    });
    if (!confirmed) return;
    runInstall();
  };

  const handleUninstall = async () => {
    const confirmed = await confirm({
      title: `Remove ${product}?`,
      description:
        `Removes ${product}: ${words.removes}. Your ${words.serves} stay on disk, but none can be ` +
        'served until it is installed again.',
      confirmLabel: 'Remove',
      variant: 'danger',
    });
    if (!confirmed) return;
    await runUninstall();
  };

  return (
    <section aria-labelledby={heading}>
      <div className="flex items-center justify-between gap-md mb-base">
        <h3 id={heading} className="m-0 text-sm font-semibold text-text">
          {product}
        </h3>
        {status && <InstallState status={status} />}
      </div>
      <p className="m-0 mb-base text-xs text-text-muted">
        {words.server}, which serves {words.serves}.
        {status && !status.installed && status.pin.targetRelease && (
          <>
            {' '}Pinned to <code className="font-mono">{status.pin.targetRelease}</code>.
          </>
        )}
      </p>

      {loadingStatus && !status && <p className="m-0 text-sm text-text-muted">Reading {product}…</p>}
      {statusError && (
        <Banner variant="danger" className="mb-base">
          {statusError}
        </Banner>
      )}
      {status?.broken && (
        <Banner variant="danger" className="mb-base">
          {status.broken}
        </Banner>
      )}

      {status?.installed && (
        <Stack gap="xs" className="mb-base">
          <LabelledValue label="Binary" value={status.binaryPath} />
          <Recorded status={status} />
          {status.reports && <LabelledValue label="Binary reports" value={status.reports} />}
          {serving && <LabelledValue label="Serving" value={serving} mono={false} />}
        </Stack>
      )}

      {status?.installed && standing?.text && (
        <p className="m-0 mb-base text-xs text-text-secondary">{standing.text}</p>
      )}
      {standing?.fromTerminal && (
        <Banner variant="info" className="mb-base">
          {standing.fromTerminal}
        </Banner>
      )}
      {(offersInstall || offersUpdate) && status?.warning && (
        <Banner variant="warning" className="mb-base">
          {status.warning}
        </Banner>
      )}
      {builtBefore && (
        <Banner variant="info" className="mb-base">
          {product} was built from source here before, so it is built again from a terminal:{' '}
          <code className="font-mono">{status?.updateCommand}</code>.
        </Banner>
      )}
      {status && !status.installed && status.downloadUnavailable && (
        <Banner variant="info" className="mb-base">
          {status.downloadUnavailable.replace(/\.$/, '')}. Build it from source with{' '}
          <code className="font-mono">{status.installCommand}</code>.
        </Banner>
      )}

      {installError && (
        <Banner variant="danger" className="mb-base">
          {installError}
        </Banner>
      )}
      {installedVersion && (
        <Banner variant="success" className="mb-base">
          {product} {installedVersion} is installed.
        </Banner>
      )}
      {installing && (
        <div className="mb-base">
          <InstallProgress progress={progress} product={product} />
        </div>
      )}
      {uninstallError && (
        <Banner variant="danger" className="mb-base">
          {uninstallError}
        </Banner>
      )}
      {uninstallResult && (
        <Banner variant="info" className="mb-base">
          {uninstallResult}
        </Banner>
      )}

      <div className="flex gap-sm">
        {offersInstall && (
          <Button
            variant="secondary"
            size="sm"
            isLoading={installing}
            disabled={busy}
            title={stopFirst}
            leftIcon={<Icon icon={Download} size={14} />}
            onClick={runInstall}
          >
            Install ({status.download})
          </Button>
        )}
        {offersUpdate && (
          <Button
            variant="secondary"
            size="sm"
            isLoading={installing}
            disabled={busy}
            title={stopFirst}
            leftIcon={<Icon icon={Download} size={14} />}
            onClick={() => void handleUpdate()}
          >
            Update to {standing?.update}
          </Button>
        )}
        {status?.installed && (
          <Button
            variant="dangerGhost"
            size="sm"
            isLoading={uninstalling}
            disabled={busy}
            title={stopFirst}
            leftIcon={<Icon icon={Trash2} size={14} />}
            onClick={() => void handleUninstall()}
          >
            Remove {product}
          </Button>
        )}
      </div>
    </section>
  );
};
