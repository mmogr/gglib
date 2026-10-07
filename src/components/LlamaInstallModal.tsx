import { FC, useEffect, useRef, useState } from 'react';
import { AlertCircle, AlertTriangle, CheckCircle2, Download, Loader2, XCircle } from 'lucide-react';
import { appLogger } from '../services/platform';
import { streamLlamaInstall } from '../services/transport/api/setup';
import type { LlamaProgressEvent } from '../types/setup';
import { InstallProgress } from './SetupWizard/InstallProgress';
import { Button } from './ui/Button';
import { Banner } from './ui/Banner';
import { Icon } from './ui/Icon';
import { Modal } from './ui/Modal';

interface LlamaInstallModalProps {
  canDownload?: boolean;
  /** Why the install state could not be read. An install's own failure replaces it. */
  error?: string | null;
  onSkip?: () => void;
  /** What a refused server start said about the missing binary. */
  metadata?: {
    expectedPath: string;
    suggestedCommand: string;
    reason: string;
  };
  onClose?: () => void;
  onInstalled?: () => void;
}

/**
 * Offers to install llama.cpp, and runs the install.
 *
 * The install is the daemon's: the stream the setup wizard reads, drawn by the
 * wizard's `InstallProgress`. So the desktop app and a browser tab install the
 * same way, and the modal is mounted only while it is shown, which is what
 * gives each opening a fresh install state.
 */
export const LlamaInstallModal: FC<LlamaInstallModalProps> = ({
  canDownload = true,
  error: statusError = null,
  onSkip,
  metadata,
  onClose,
  onInstalled,
}) => {
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<LlamaProgressEvent | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const stopReading = useRef<(() => void) | null>(null);

  // Unmounting stops the reading, not the install: the daemon finishes it.
  useEffect(() => () => stopReading.current?.(), []);

  const isCompleted = progress?.type === 'completed';
  const error = installError ?? statusError;

  const handleInstall = () => {
    // A stream ends once: with `completed`, with `failed`, with a transport
    // error, or by closing having said none of them. Whichever comes first is
    // the result, and `installing` must not outlive it, because the modal
    // cannot be closed while it is set.
    let settled = false;
    const fail = (message: string) => {
      if (settled) return;
      settled = true;
      appLogger.error('component.settings', 'llama.cpp install failed', { message });
      setInstalling(false);
      setInstallError(message);
    };

    setInstalling(true);
    setInstallError(null);
    setProgress(null);

    stopReading.current = streamLlamaInstall(
      (event) => {
        if (settled) return;
        setProgress(event);
        if (event.type === 'completed') {
          settled = true;
          setInstalling(false);
          onInstalled?.();
          // Opened by a refused server start, there is nothing left to show.
          if (metadata) onClose?.();
        } else if (event.type === 'failed') {
          fail(event.message);
        }
      },
      fail,
      () => fail('The connection to the install ended before it reported a result.'),
    );
  };

  const renderFooterContent = () => {
    if (metadata) {
      return (
        <>
          <Button onClick={handleInstall} disabled={installing} leftIcon={<Icon icon={Download} size={16} />}>
            Install now
          </Button>
          <Button variant="ghost" onClick={onClose} disabled={installing}>
            Cancel
          </Button>
        </>
      );
    }
    if (!installing && !isCompleted && canDownload) {
      return (
        <>
          <Button onClick={handleInstall} disabled={installing} leftIcon={<Icon icon={Download} size={16} />}>
            Install llama.cpp
          </Button>
          {onSkip ? (
            <Button variant="ghost" onClick={onSkip} disabled={installing}>
              Skip for now
            </Button>
          ) : null}
        </>
      );
    }
    if (!installing && !isCompleted && !canDownload && onSkip) {
      return (
        <Button variant="ghost" onClick={onSkip}>
          I understand
        </Button>
      );
    }
    return null;
  };
  const renderMetadataContent = () => (
    <>
      <div className="flex gap-3 items-center">
        <div className="w-10 h-10 rounded-full inline-flex items-center justify-center bg-background-secondary border border-border text-primary">
          <Icon icon={installing ? Loader2 : AlertTriangle} size={28} className={installing ? 'animate-spin' : ''} />
        </div>
        <div>
          <h2 className="m-0 text-lg font-semibold text-text">llama-server Not Installed</h2>
          <p className="mt-1 mb-0 text-text-secondary text-base">{metadata?.reason}</p>
        </div>
      </div>

      <div className="flex flex-col gap-2">
        <p className="m-0 text-text-secondary leading-normal">The llama-server binary was not found at:</p>
        <code className="block bg-background-tertiary border border-border rounded-md py-2 px-3 font-mono text-base text-text break-all">{metadata?.expectedPath}</code>
      </div>

      {error ? <Banner variant="danger">{error}</Banner> : null}

      {installing ? <InstallProgress progress={progress} /> : null}
    </>
  );

  const renderStandardContent = () => (
    <>
      <div className="flex gap-3 items-center">
        <div className="w-10 h-10 rounded-full inline-flex items-center justify-center bg-background-secondary border border-border text-primary">
          <Icon
            icon={isCompleted ? CheckCircle2 : installError ? XCircle : AlertCircle}
            size={28}
            className={installing ? 'animate-pulse' : ''}
          />
        </div>
        <div>
          <h2 className="m-0 text-lg font-semibold text-text">{isCompleted ? 'Installation complete' : 'llama.cpp required'}</h2>
          {!installing && !isCompleted && (
            <p className="mt-1 mb-0 text-text-secondary text-base">
              {canDownload
                ? 'We will download a prebuilt binary for your platform (~15 MB).'
                : 'Please build llama.cpp via the CLI: gglib config llama install'}
            </p>
          )}
        </div>
      </div>

      {error && !installing ? <Banner variant="danger">{error}</Banner> : null}

      {installing ? <InstallProgress progress={progress} /> : null}

      {progress?.type === 'completed' ? (
        <p className="text-success font-semibold">llama.cpp {progress.version} is ready! You can now serve models.</p>
      ) : null}
    </>
  );

  return (
    <Modal
      open
      onClose={onClose ?? (() => {})}
      title="Llama installation"
      size="md"
      preventClose={installing}
      footer={renderFooterContent() ?? undefined}
    >
      <div className="flex flex-col gap-4">{metadata ? renderMetadataContent() : renderStandardContent()}</div>
    </Modal>
  );
};

export default LlamaInstallModal;
