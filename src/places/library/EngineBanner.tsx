/**
 * The line above the library's groups that says llama.cpp needs installing
 * or updating, and does it: shown when the daemon's setup status says
 * llama.cpp is missing, or older than the pinned release with an update
 * that downloads (`useLlamaStatus`), a model running on it or not. Not a
 * dialog: the library stays usable under it, and "Not now" puts it away for
 * as long as the window is open (`engineInstall`).
 *
 * Install and Update run what the setup wizard runs, the daemon's install
 * stream, with its progress drawn in the banner; the install outlives the
 * banner. A refusal is the daemon's sentence, shown as it came: it will not
 * replace llama-server under a model running on it, and says which model to
 * stop. A completed install reads the status again.
 *
 * @module places/library/EngineBanner
 */

import { useEffect, useRef, type FC } from 'react';
import { StatusMark } from '../../components/status/StatusMark';
import { InstallProgress } from '../../components/SetupWizard/InstallProgress';
import { Button } from '../../components/ui/Button';
import { useLlamaStatus } from '../../hooks/useLlamaStatus';
import { dismissEngineBanner, installLlama, useEngineInstall } from './engineInstall';

const INSTALL_COMMAND = 'gglib config llama install';

export const EngineBanner: FC = () => {
  const { status, checkStatus } = useLlamaStatus();
  const { dismissed, installing, progress, refusal, completed } = useEngineInstall();

  // An install completed while the banner was up: read the status again.
  const seen = useRef(completed);
  useEffect(() => {
    if (completed === seen.current) return;
    seen.current = completed;
    void checkStatus();
  }, [completed, checkStatus]);

  if (status === null || dismissed) return null;
  const update = status.installed ? status.update : null;
  if (status.installed && update === null) return null;

  const sentence = update
    ? `llama.cpp ${update.installed} is older than the ${update.target} this gglib expects.`
    : status.canDownload
      ? 'llama.cpp is not installed.'
      : `llama.cpp is not installed. Build it from a terminal: ${INSTALL_COMMAND}`;
  const canRun = update !== null || status.canDownload;

  return (
    <div role="status" aria-label="llama.cpp" className="mx-md mb-sm flex flex-col gap-sm rounded-base bg-surface-elevated px-md py-sm">
      <div className="flex flex-wrap items-center gap-sm text-sm text-text">
        <StatusMark kind="attention" label="Attention" />
        <span className="min-w-0 flex-1">{sentence}</span>
        {canRun && !installing && (
          <Button size="sm" variant="secondary" onClick={installLlama}>
            {update ? 'Update' : 'Install'}
          </Button>
        )}
        {!installing && (
          <Button size="sm" variant="ghost" onClick={dismissEngineBanner}>
            Not now
          </Button>
        )}
      </div>
      {installing && <InstallProgress progress={progress} />}
      {refusal && <p className="m-0 text-xs text-text-secondary">{refusal}</p>}
    </div>
  );
};
