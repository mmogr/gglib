/**
 * The line above the library's groups that says llama.cpp needs installing
 * or updating, and does it: shown only when the daemon's setup status says
 * llama.cpp is missing, or older than the pinned release with an update
 * that downloads (`useLlamaStatus`). Not a dialog: the library stays usable
 * under it, and "Not now" puts it away until the next launch.
 *
 * Install and Update run what the install dialog runs, the daemon's install
 * stream, with its progress drawn in the banner. A refusal is the daemon's
 * sentence, shown as it came: it will not replace llama-server under a model
 * running on it, and says which model to stop.
 *
 * @module places/library/EngineBanner
 */

import { useEffect, useRef, useState, type FC } from 'react';
import { StatusMark } from '../../components/status/StatusMark';
import { InstallProgress } from '../../components/SetupWizard/InstallProgress';
import { Button } from '../../components/ui/Button';
import { useLlamaStatus } from '../../hooks/useLlamaStatus';
import { streamEngineInstall } from '../../services/transport/api/setup';
import type { LlamaProgressEvent } from '../../types/setup';

const INSTALL_COMMAND = 'gglib config llama install';

export const EngineBanner: FC = () => {
  const { status, checkStatus } = useLlamaStatus();
  const [dismissed, setDismissed] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<LlamaProgressEvent | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);
  const stopReading = useRef<(() => void) | null>(null);

  // Leaving stops the reading, not the install: the daemon finishes it.
  useEffect(() => () => stopReading.current?.(), []);

  if (status === null || dismissed) return null;
  const update = status.installed ? status.update : null;
  if (status.installed && update === null) return null;

  const sentence = update
    ? `llama.cpp ${update.installed} is older than the ${update.target} this gglib expects.`
    : status.canDownload
      ? 'llama.cpp is not installed.'
      : `llama.cpp is not installed. Build it from a terminal: ${INSTALL_COMMAND}`;
  const canRun = update !== null || status.canDownload;

  const install = () => {
    // The stream ends once: completed, failed, a transport error, or closed
    // having said neither. Whichever comes first is the result.
    let settled = false;
    const fail = (message: string) => {
      if (settled) return;
      settled = true;
      setInstalling(false);
      setRefusal(message);
    };
    setInstalling(true);
    setRefusal(null);
    setProgress(null);
    stopReading.current = streamEngineInstall(
      'llama',
      (event) => {
        if (settled) return;
        setProgress(event);
        if (event.type === 'completed') {
          settled = true;
          setInstalling(false);
          void checkStatus();
        } else if (event.type === 'failed') {
          fail(event.message);
        }
      },
      fail,
      () => fail('The connection to the install ended before it reported a result.'),
    );
  };

  return (
    <div role="status" aria-label="llama.cpp" className="mx-md mb-sm flex flex-col gap-sm rounded-base bg-surface-elevated px-md py-sm">
      <div className="flex flex-wrap items-center gap-sm text-sm text-text">
        <StatusMark kind="attention" label="Attention" />
        <span className="min-w-0 flex-1">{sentence}</span>
        {canRun && !installing && (
          <Button size="sm" variant="secondary" onClick={install}>
            {update ? 'Update' : 'Install'}
          </Button>
        )}
        {!installing && (
          <Button size="sm" variant="ghost" onClick={() => setDismissed(true)}>
            Not now
          </Button>
        )}
      </div>
      {installing && <InstallProgress progress={progress} />}
      {refusal && <p className="m-0 text-xs text-text-secondary">{refusal}</p>}
    </div>
  );
};
