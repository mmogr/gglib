import { FC, useEffect, useRef, useState } from 'react';
import { AlertTriangle, Download, Loader2 } from 'lucide-react';
import { appLogger } from '../services/platform';
import { streamEngineInstall } from '../services/transport/api/setup';
import type { LlamaProgressEvent } from '../types/setup';
import { InstallProgress } from './SetupWizard/InstallProgress';
import { Button } from './ui/Button';
import { Banner } from './ui/Banner';
import { Icon } from './ui/Icon';
import { Modal } from './ui/Modal';

interface LlamaInstallModalProps {
  /** What a refused server start said about the missing binary. */
  metadata: {
    expectedPath: string;
    suggestedCommand: string;
    reason: string;
  };
  onClose: () => void;
}

/**
 * Offers to install llama.cpp when a server start found no llama-server,
 * runs the install, and closes once it has completed.
 *
 * The install is the daemon's: the stream the setup wizard reads, drawn by the
 * wizard's `InstallProgress`. So the desktop app and a browser tab install the
 * same way, and the modal is mounted only while it is shown, which is what
 * gives each opening a fresh install state. A missing or older llama.cpp at
 * launch is the library's banner (`places/library/EngineBanner`), not this.
 */
export const LlamaInstallModal: FC<LlamaInstallModalProps> = ({ metadata, onClose }) => {
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<LlamaProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const stopReading = useRef<(() => void) | null>(null);

  // Unmounting stops the reading, not the install: the daemon finishes it.
  useEffect(() => () => stopReading.current?.(), []);

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
      setError(message);
    };

    setInstalling(true);
    setError(null);
    setProgress(null);

    stopReading.current = streamEngineInstall(
      'llama',
      (event) => {
        if (settled) return;
        setProgress(event);
        if (event.type === 'completed') {
          settled = true;
          setInstalling(false);
          onClose();
        } else if (event.type === 'failed') {
          fail(event.message);
        }
      },
      fail,
      () => fail('The connection to the install ended before it reported a result.'),
    );
  };

  return (
    <Modal
      open
      onClose={onClose}
      title="Llama installation"
      size="md"
      preventClose={installing}
      footer={
        <>
          <Button onClick={handleInstall} disabled={installing} leftIcon={<Icon icon={Download} size={16} />}>
            Install now
          </Button>
          <Button variant="ghost" onClick={onClose} disabled={installing}>
            Cancel
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <div className="flex gap-3 items-center">
          <div className="w-10 h-10 rounded-full inline-flex items-center justify-center bg-background-secondary border border-border text-primary">
            <Icon icon={installing ? Loader2 : AlertTriangle} size={28} className={installing ? 'animate-spin' : ''} />
          </div>
          <div>
            <h2 className="m-0 text-base font-semibold text-text">llama-server Not Installed</h2>
            <p className="mt-1 mb-0 text-text-secondary text-sm">{metadata.reason}</p>
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <p className="m-0 text-text-secondary leading-normal">The llama-server binary was not found at:</p>
          <code className="block bg-background-tertiary border border-border rounded-md py-2 px-3 font-mono text-sm text-text break-all">{metadata.expectedPath}</code>
        </div>

        {error ? <Banner variant="danger">{error}</Banner> : null}

        {installing ? <InstallProgress progress={progress} /> : null}
      </div>
    </Modal>
  );
};

export default LlamaInstallModal;
