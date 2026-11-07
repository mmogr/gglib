/**
 * Settings › Engines › Faster downloads: the optional hf_xet accelerator,
 * the GUI face of `gglib config fast-downloads status|enable|disable`.
 *
 * Off offers Set up, which builds the accelerator's own Python environment
 * on the daemon (it can take a minute); On offers ⋯ Turn off, after which
 * downloads run natively over HTTP again. Both read the state back after.
 * Where the daemon finds no Python to build it with, there is no Set up:
 * the row names the command that builds it once one is installed, with a
 * ⧉. A failure is said in the daemon's words, which name the remedy.
 *
 * The state is the daemon's setup status: `fastDownloadReady` (the same
 * check that picks a download's backend) and `pythonAvailable` (its
 * preflight). It is the one route that says whether Python is there.
 *
 * @module places/settings/FasterDownloads
 */

import { useCallback, useEffect, useState, type FC } from 'react';
import { MoreHorizontal } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { CopyValue } from '../../components/CopyValue';
import { Icon } from '../../components/ui/Icon';
import { StatusMark } from '../../components/status/StatusMark';
import { useToastContext } from '../../contexts/ToastContext';
import { disableFastDownloads, enableFastDownloads, getSetupStatus } from '../../services/transport/api/setup';
import { formatError } from '../../utils/errors';
import { Menu } from '../library/Menu';

/** The command that builds the accelerator from a terminal (`gglib config fast-downloads enable`). */
const ENABLE_COMMAND = 'gglib config fast-downloads enable';

interface Accelerator {
  on: boolean;
  python: boolean;
}

export const FasterDownloads: FC = () => {
  const { showToast } = useToastContext();
  const [state, setState] = useState<Accelerator | null>(null);
  const [changing, setChanging] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const read = useCallback(
    () =>
      getSetupStatus()
        .then((status) => setState({ on: status.fastDownloadReady, python: status.pythonAvailable }))
        .catch((err: unknown) => setError(formatError(err))),
    [],
  );

  useEffect(() => {
    void read();
  }, [read]);

  const change = async (on: boolean) => {
    setChanging(true);
    setError(null);
    try {
      if (on) await enableFastDownloads();
      else await disableFastDownloads();
      // Read back before the busy mark goes, so the row does not flick to
      // the state it was leaving.
      await read();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setChanging(false);
    }
  };

  return (
    <section aria-labelledby="faster-downloads-heading" className="flex flex-col gap-sm">
      <div className="flex items-start justify-between gap-md">
        <div className="flex min-w-0 flex-col gap-xs">
          <h3 id="faster-downloads-heading" className="m-0 text-sm font-semibold text-text">
            Faster downloads
          </h3>
          <p className="m-0 text-xs text-text-secondary">
            Hugging Face's hf_xet transfer, quicker than plain HTTP for large files. Downloads work without it.
          </p>
          {state && <StatusMark kind={state.on ? 'on' : 'off'} word={state.on ? 'On' : 'Off'} className="text-xs" />}
        </div>
        <div className="flex shrink-0 items-center gap-xs">
          {state && !state.on && state.python && (
            <Button variant="secondary" size="sm" disabled={changing} onClick={() => void change(true)}>
              Set up
            </Button>
          )}
          {state?.on && (
            <Menu
              iconOnly
              name="More for Faster downloads"
              label={<Icon icon={MoreHorizontal} size={14} />}
              items={[{ label: 'Turn off', disabled: changing, onPick: () => void change(false) }]}
            />
          )}
        </div>
      </div>

      {changing && state && !state.on && (
        <p className="m-0 text-xs text-text-secondary">Setting up. This can take a minute.</p>
      )}
      {state && !state.on && !state.python && (
        <div className="flex flex-wrap items-center gap-xs text-xs text-text-secondary">
          <span>It needs Python 3.10 or newer. Once that is installed:</span>
          <code className="font-mono">{ENABLE_COMMAND}</code>
          <CopyValue
            value={ENABLE_COMMAND}
            label="Copy the command"
            onCopied={() => showToast('Copied the command.', 'success')}
          />
        </div>
      )}
      {error && (
        <p role="alert" className="m-0 text-xs text-danger">
          {error}
        </p>
      )}
    </section>
  );
};
