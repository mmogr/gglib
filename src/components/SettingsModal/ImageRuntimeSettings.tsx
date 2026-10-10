/**
 * The image runtime section of System settings: the web face of
 * `gglib config sd install|status|uninstall`.
 *
 * stable-diffusion.cpp's `sd-server` is what gglib draws with. This says
 * whether it is installed and what it is, installs the pinned pre-built build
 * with the setup wizard's progress, warns when that build runs on the CPU
 * alone, and removes it, never from under a running image model. Where no
 * pre-built build fits the machine it names the command that builds one.
 */

import type { FC } from 'react';
import { Download, Trash2 } from 'lucide-react';
import { Button } from '../ui/Button';
import { Banner } from '../ui/Banner';
import { Icon } from '../ui/Icon';
import { Stack } from '../primitives';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { InstallProgress } from '../SetupWizard/InstallProgress';
import type { ImageRuntimeStatus } from '../../types/setup';
import { LabelledValue } from './LabelledValue';
import { useImageRuntime } from './useImageRuntime';

/** The product the install's labels name. */
const PRODUCT = 'stable-diffusion.cpp';

/** Installed / absent, as a dot and a word. */
const InstallState: FC<{ status: ImageRuntimeStatus }> = ({ status }) => {
  const [dot, label] = status.install.installed
    ? ['bg-success', 'Installed']
    : ['bg-text-muted', 'Not installed'];
  return (
    <div className="flex items-center gap-xs">
      <span className={`inline-block w-2 h-2 rounded-full ${dot}`} aria-hidden="true" />
      <span className="text-sm font-semibold text-text">{label}</span>
    </div>
  );
};

export const ImageRuntimeSettings: FC = () => {
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
  } = useImageRuntime();
  const { confirm } = useConfirmContext();

  const handleUninstall = async () => {
    const confirmed = await confirm({
      title: 'Remove the image runtime?',
      description:
        'Removes stable-diffusion.cpp: sd-server, its library and its install record. Your ' +
        'image models stay on disk, but none can be served until it is installed again.',
      confirmLabel: 'Remove',
      variant: 'danger',
    });
    if (!confirmed) return;
    await runUninstall();
  };

  const install = status?.install;
  const running = status?.runningModel ?? null;

  return (
    <section aria-labelledby="image-runtime-heading">
      <div className="flex items-center justify-between gap-md mb-base">
        <h3 id="image-runtime-heading" className="m-0 text-sm font-semibold text-text">
          Image runtime
        </h3>
        {status && <InstallState status={status} />}
      </div>
      <p className="m-0 mb-base text-xs text-text-muted">
        stable-diffusion.cpp&apos;s sd-server, which serves image models. Pinned to{' '}
        <code className="font-mono">{install?.pinnedRelease ?? '…'}</code>.
      </p>

      {loadingStatus && !status && (
        <p className="m-0 text-sm text-text-muted">Reading the image runtime…</p>
      )}
      {statusError && (
        <Banner variant="danger" className="mb-base">
          {statusError}
        </Banner>
      )}

      {install?.installed && (
        <Stack gap="xs" className="mb-base">
          <LabelledValue label="Binary" value={install.binaryPath} />
          {install.release && <LabelledValue label="Release" value={install.release} />}
          {install.platform && <LabelledValue label="Build" value={install.platform} mono={false} />}
          {install.recordError && (
            <LabelledValue label="Install record" value={install.recordError} mono={false} />
          )}
          {install.versionLine && <LabelledValue label="Binary reports" value={install.versionLine} />}
          {running && <LabelledValue label="Serving" value={running} mono={false} />}
        </Stack>
      )}

      {status?.warning && (
        <Banner variant="warning" className="mb-base">
          {status.warning}
        </Banner>
      )}

      {status && !install?.installed && status.prebuiltUnavailable && (
        <Banner variant="info" className="mb-base">
          {status.prebuiltUnavailable}. Build it from source with{' '}
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
          stable-diffusion.cpp {installedVersion} is installed.
        </Banner>
      )}
      {installing && (
        <div className="mb-base">
          <InstallProgress progress={progress} product={PRODUCT} />
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
        {status && !install?.installed && status.prebuilt && (
          <Button
            variant="secondary"
            size="sm"
            isLoading={installing}
            disabled={installing || uninstalling}
            leftIcon={<Icon icon={Download} size={14} />}
            onClick={runInstall}
          >
            Install ({status.prebuilt})
          </Button>
        )}
        {install?.installed && (
          <Button
            variant="dangerGhost"
            size="sm"
            isLoading={uninstalling}
            disabled={uninstalling || installing || running !== null}
            title={running ? `Stop ${running} first.` : undefined}
            leftIcon={<Icon icon={Trash2} size={14} />}
            onClick={() => void handleUninstall()}
          >
            Remove the image runtime
          </Button>
        )}
      </div>
    </section>
  );
};
