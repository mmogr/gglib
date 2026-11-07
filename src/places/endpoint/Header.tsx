/**
 * The endpoint's address line and what can be done to it.
 *
 * On: `● On`, the address a client reaches it at (the daemon's `base_url`)
 * with Copy, the one line of what it means, Test it and Turn off. Pinned:
 * `● Pinned`, and in place of the meaning the one model it serves, with
 * Unpin. A line only when true: Key required, with Show (the key is
 * revealed only when asked, EP-5). Off: `○ Off`, Turn on, what being off
 * means, and Start when gglib launches (the stored setting). Starting: ◌.
 *
 * Test it says "Testing…" at once (a proof waits while the model loads),
 * then who answered or why not.
 *
 * A failed start or stop is said in the daemon's words (EP-1), whether it
 * names a port in use, llama.cpp missing or anything else: nothing here
 * detects those. Shared on your network is not said: the status reaches a
 * wildcard bind on loopback, so no fact of the run says it is shared.
 *
 * @module places/endpoint/Header
 */

import { useState, type FC } from 'react';
import { KeyReveal } from '../../components/ConnectClient';
import { StatusMark, type MarkKind } from '../../components/status/StatusMark';
import { ENDPOINT_WORDS, KEY_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
import { useToastContext } from '../../contexts/ToastContext';
import { useSettings } from '../../hooks/useSettings';
import type { EndpointFacts } from '../../services/proxyRegistry';
import type { EndpointState } from '../../types/generated/EndpointState';
import { formatError } from '../../utils/errors';
import type { EndpointActions } from './useEndpointActions';

const MARK: Record<EndpointState, MarkKind> = { on: 'on', pinned: 'on', starting: 'changing', off: 'off' };

interface HeaderProps {
  facts: EndpointFacts;
  actions: EndpointActions;
  /** Test it: one real request for the model Connect a client names. */
  onTest: () => void;
}

export const Header: FC<HeaderProps> = ({ facts, actions, onTest }) => {
  const { showToast } = useToastContext();
  const { settings, save } = useSettings();
  const [saveError, setSaveError] = useState<string | null>(null);
  const state: EndpointState = actions.busy === 'on' || actions.busy === 'serve' ? 'starting' : facts.state;
  const up = state === 'on' || state === 'pinned';

  const copy = (url: string) =>
    void navigator.clipboard
      .writeText(url)
      .then(() => showToast('Endpoint address copied', 'success'))
      .catch((err: unknown) => showToast(formatError(err), 'error'));

  return (
    <div className="flex flex-col gap-xs px-md pb-md">
      <div className="flex items-center gap-md">
        <StatusMark kind={MARK[state]} word={ENDPOINT_WORDS[state]} className="text-base text-text" />
        {up && facts.baseUrl !== null && (
          <>
            <span className="min-w-0 flex-1 truncate font-mono text-xl text-text" title={facts.baseUrl}>
              {facts.baseUrl}
            </span>
            <Button onClick={() => copy(facts.baseUrl ?? '')}>Copy</Button>
          </>
        )}
        {state === 'off' && (
          <Button onClick={() => void actions.turnOn()} isLoading={actions.busy === 'on'}>
            Turn on
          </Button>
        )}
      </div>

      {up && (
        <div className="flex items-center gap-md pl-lg">
          <p className="m-0 flex-1 text-sm text-text-secondary">
            {facts.pinnedModel !== null
              ? `only ${facts.pinnedModel}, other names are refused`
              : 'Every model in your library answers here by name.'}
          </p>
          {facts.pinnedModel !== null && (
            <Button variant="ghost" size="sm" onClick={() => void actions.serve(null)}>
              Unpin
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={onTest} isLoading={actions.testing}>
            Test it
          </Button>
          <Button variant="ghost" size="sm" onClick={() => void actions.turnOff()} isLoading={actions.busy === 'off'}>
            Turn off
          </Button>
        </div>
      )}
      {up && facts.key === 'required' && (
        <p className="m-0 flex items-center gap-sm pl-lg text-xs text-text-secondary">
          <span>{`Key ${KEY_WORDS.required.toLowerCase()}`}</span>
          <KeyReveal />
        </p>
      )}

      {state === 'off' && (
        <div className="flex flex-col gap-sm pl-lg">
          <p className="m-0 text-sm text-text-secondary">
            Apps can&apos;t reach your models while it&apos;s off. Loaded models stay in memory.
          </p>
          <Checkbox
            label="Start when gglib launches"
            checked={settings?.proxyAutostart === true}
            disabled={settings === null}
            onChange={(e) => {
              setSaveError(null);
              void save({ proxyAutostart: e.target.checked }).catch((err: unknown) => setSaveError(formatError(err)));
            }}
          />
          {saveError !== null && (
            <p role="alert" className="m-0 flex items-baseline gap-xs text-xs text-text">
              <StatusMark kind="failed" label="Failed" />
              {saveError}
            </p>
          )}
        </div>
      )}

      {actions.error !== null && (
        <p role="alert" className="m-0 flex items-baseline gap-xs pl-lg text-xs text-text">
          <StatusMark kind="failed" label="Failed" />
          {actions.error}
        </p>
      )}
      {actions.testing && (
        <p role="status" className="m-0 flex items-baseline gap-xs pl-lg text-xs text-text-secondary">
          <StatusMark kind="changing" label="Testing" />
          Testing…
        </p>
      )}
      {actions.proof !== null && (
        <p role="status" className="m-0 flex items-baseline gap-xs pl-lg text-xs text-text">
          <StatusMark kind={actions.proof.answered ? 'on' : 'failed'} label={actions.proof.answered ? 'Answered' : 'Failed'} />
          {actions.proof.sentence}
        </p>
      )}
    </div>
  );
};
