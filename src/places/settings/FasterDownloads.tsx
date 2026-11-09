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
 * The state is the daemon's setup status: `fastDownloads` (the same
 * check that picks a download's backend, as the state `FAST_DOWNLOADS_WORDS`
 * words) and `pythonAvailable` (its preflight). It is the one route that says whether Python is there.
 *
 * @module places/settings/FasterDownloads
 */

import { useCallback, useEffect, useState, type FC } from 'react';
import { MoreHorizontal } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { CopyValue } from '../../components/CopyValue';
import { Icon } from '../../components/ui/Icon';
import { StatusMark, type MarkKind } from '../../components/status/StatusMark';
import { FAST_DOWNLOADS_WORDS } from '../../components/status/words';
import { disableFastDownloads, enableFastDownloads, getSetupStatus } from '../../services/transport/api/setup';
import { formatError } from '../../utils/errors';
import { Menu } from '../../components/ui/Menu';
import type { FastDownloadsState } from '../../types/generated/FastDownloadsState';

/** The command that builds the accelerator from a terminal (`gglib config fast-downloads enable`). */
const ENABLE_COMMAND = 'gglib config fast-downloads enable';

/** The shape each state is drawn as; its word is `FAST_DOWNLOADS_WORDS`'. */
const MARK: Record<FastDownloadsState, MarkKind> = { on: 'on', off: 'off' };

interface Accelerator {
  state: FastDownloadsState;
  python: boolean;
}

export const FasterDownloads: FC = () => {
  const [state, setState] = useState<Accelerator | null>(null);
  const [changing, setChanging] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const read = useCallback(
    () =>
      getSetupStatus()
        .then((status) => setState({ state: status.fastDownloads, python: status.pythonAvailable }))
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
          {state && <StatusMark kind={MARK[state.state]} word={FAST_DOWNLOADS_WORDS[state.state]} className="text-xs" />}
        </div>
        <div className="flex shrink-0 items-center gap-xs">
          {state?.state === 'off' && state.python && (
            <Button variant="secondary" size="sm" disabled={changing} onClick={() => void change(true)}>
              Set up
            </Button>
          )}
          {state?.state === 'on' && (
            <Menu
              iconOnly
              name="More for Faster downloads"
              label={<Icon icon={MoreHorizontal} size={14} />}
              items={[{ label: 'Turn off', disabled: changing, onPick: () => void change(false) }]}
            />
          )}
        </div>
      </div>

      {changing && state?.state === 'off' && (
        <p className="m-0 text-xs text-text-secondary">Setting up. This can take a minute.</p>
      )}
      {state?.state === 'off' && !state.python && (
        <div className="flex flex-wrap items-center gap-xs text-xs text-text-secondary">
          <span>It needs Python 3.10 or newer. Once that is installed:</span>
          <code className="font-mono">{ENABLE_COMMAND}</code>
          <CopyValue value={ENABLE_COMMAND} label="Copy the command" />
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
