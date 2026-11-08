/**
 * Setup's step 4, Endpoint: turned on when this step is first reached, its
 * address with Copy, then the proof, one real request to the first model,
 * in the words `gglib proxy test` prints (`readings/connect`).
 *
 * The proof waits for the model's download ("The test runs when … finishes")
 * and is run by the place, so it lands on whichever step is open; nothing
 * here waits for it, and Continue is always there. Start the endpoint when
 * gglib launches is the stored setting (`proxyAutostart`). A failed start
 * or proof is the daemon's sentence, with Retry. An endpoint that was on
 * before setup, with a model already here, reads "Already done".
 *
 * @module places/setup/EndpointStep
 */

import { useState, type FC } from 'react';
import { Copied, useCopied } from '../../components/CopyValue';
import { StatusMark, type MarkKind } from '../../components/status/StatusMark';
import { ENDPOINT_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
import { useToastContext } from '../../contexts/ToastContext';
import { useSettings } from '../../hooks/useSettings';
import type { EndpointFacts } from '../../services/proxyRegistry';
import type { EndpointState } from '../../types/generated/EndpointState';
import { formatError } from '../../utils/errors';
import type { EndpointActions } from '../endpoint/useEndpointActions';
import { Problem, Step, useAdvance } from './Step';

const MARK: Record<EndpointState, MarkKind> = { on: 'on', pinned: 'on', starting: 'changing', off: 'off' };

interface EndpointStepProps {
  facts: EndpointFacts;
  actions: EndpointActions;
  alreadyDone: boolean;
  advance: (() => void) | null;
  /** The download the proof waits for, by name; `null` when it waits for none. */
  waitingOn: string | null;
  onRetryProof: () => void;
  onNext: () => void;
  onBack: () => void;
}

export const EndpointStep: FC<EndpointStepProps> = ({
  facts,
  actions,
  alreadyDone,
  advance,
  waitingOn,
  onRetryProof,
  onNext,
  onBack,
}) => {
  useAdvance(alreadyDone, advance);
  const { showToast } = useToastContext();
  const [copied, showCopied] = useCopied();
  const { settings, save } = useSettings();
  const [saveError, setSaveError] = useState<string | null>(null);
  const state: EndpointState = actions.busy === 'on' ? 'starting' : facts.state;
  const up = state === 'on' || state === 'pinned';

  const copy = (url: string) =>
    void navigator.clipboard
      .writeText(url)
      .then(showCopied)
      .catch((err: unknown) => showToast(formatError(err), 'error'));

  return (
    <Step n={4} title="Endpoint" onBack={onBack} actions={<Button onClick={onNext}>Continue</Button>}>
      {alreadyDone && <StatusMark kind="on" word="Already done" />}
      <div className="flex flex-wrap items-center gap-md">
        <StatusMark kind={MARK[state]} word={ENDPOINT_WORDS[state]} className="text-text" />
        {up && facts.baseUrl !== null && (
          <>
            <code className="min-w-0 truncate font-mono text-sm text-text">{facts.baseUrl}</code>
            <Button variant="ghost" size="sm" onClick={() => copy(facts.baseUrl ?? '')}>
              Copy
            </Button>
            <Copied shown={copied} />
          </>
        )}
      </div>
      {actions.error !== null && <Problem onRetry={() => void actions.turnOn()}>{actions.error}</Problem>}
      {!alreadyDone && waitingOn !== null && actions.proof === null && !actions.testing && (
        <p className="m-0 text-xs text-text-secondary">{`The test runs when ${waitingOn} finishes.`}</p>
      )}
      {actions.testing && <StatusMark kind="changing" word="Testing…" className="text-xs" />}
      {actions.proof?.answered && <StatusMark kind="on" word={actions.proof.sentence} className="text-xs text-text" />}
      {actions.proof && !actions.proof.answered && <Problem onRetry={onRetryProof}>{actions.proof.sentence}</Problem>}
      <Checkbox
        label="Start the endpoint when gglib launches"
        checked={settings?.proxyAutostart === true}
        disabled={settings === null}
        onChange={(e) => {
          setSaveError(null);
          void save({ proxyAutostart: e.target.checked }).catch((err: unknown) => setSaveError(formatError(err)));
        }}
      />
      {saveError !== null && <Problem>{saveError}</Problem>}
    </Step>
  );
};
