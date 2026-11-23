/**
 * Setup's step 3, First model: the model gglib suggests for this machine,
 * from the daemon's shortlist (`GET /api/config/system/recommend-model`,
 * the one `gglib up` picks from), named by its repository with its own
 * reason, the size its download takes and the daemon's sentence for a load
 * of that size on this machine, as the Library's suggestion says it.
 *
 * Get it sets up faster downloads first where they are to be
 * (`useFastDownloads`), then queues that size, and its row's own words follow it (`2.1 GiB / 5.0 GiB ·
 * 52.0 MB/s · ETA 1m 0s`, TX-13), a note such as a reconnect leading them; Continue does not
 * wait for it. A download that ended without the model says so, in its ending's words, with
 * Retry, which queues it again: one that gave up on a quiet connection or was cut off carries
 * on from its part on disk, and one whose bytes failed their check starts over. One
 * Hugging Face's rate limit held reads as a running download that waits does
 * (`waiting for Hugging Face's rate limit, 2:14`) until it is queued. Choose
 * another goes on and leaves the choice to the Library's search once setup
 * is done; Skip goes on with no model. "Use another machine's models ›" opens a
 * pairing field, joined as the Devices place joins. A library that already
 * has a chat model reads "Already done" and moves on.
 *
 * @module places/setup/FirstModelStep
 */

import { useState, type FC } from 'react';
import { waitingSentence } from '../../components/status/hubWait';
import { StatusMark } from '../../components/status/StatusMark';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useSecondsLeft } from '../../hooks/useSecondsLeft';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { getTransport } from '../../services/transport';
import { formatError } from '../../utils/errors';
import { formatRecommendation } from '../../utils/format';
import type { RecommendationDto } from '../../types/setup';
import { Item, Problem, Step, useAdvance } from './Step';
import type { FirstModel } from './useFirstModel';

/** Another machine's models: its pairing string, joined. */
const Join: FC<{ onJoined: () => void }> = ({ onJoined }) => {
  const [open, setOpen] = useState(false);
  const [pairing, setPairing] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [joined, setJoined] = useState(false);

  const join = async (text: string) => {
    setBusy(true);
    setError(null);
    try {
      await getTransport().joinRemote({ pairing: text });
      void refreshRemoteStatus();
      setJoined(true);
      onJoined();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setBusy(false);
    }
  };

  if (joined) return <StatusMark kind="on" word="Joined. Its models are in your Library." />;
  if (!open) {
    return (
      <Button variant="link" size="sm" className="self-start text-sm font-normal" onClick={() => setOpen(true)}>
        Use another machine&apos;s models ›
      </Button>
    );
  }
  return (
    <form
      className="flex flex-col gap-xs"
      onSubmit={(event) => {
        event.preventDefault();
        if (pairing.trim()) void join(pairing.trim());
      }}
    >
      <div className="flex items-center gap-sm">
        <Input
          aria-label="Paste a pairing string"
          placeholder="Paste a pairing string"
          className="min-w-0 flex-1 font-mono"
          value={pairing}
          onChange={(event) => {
            setPairing(event.target.value);
            setError(null);
          }}
        />
        {busy ? (
          <StatusMark kind="changing" word="Reaching it…" />
        ) : (
          <Button type="submit" variant="secondary" size="sm" disabled={!pairing.trim()}>
            Join
          </Button>
        )}
      </div>
      {error !== null && <Problem>{error}</Problem>}
    </form>
  );
};

/** The download's line: the wait while it is held, its row's words while it runs, how it ended once it has. */
const Download: FC<{ first: FirstModel; onRetry: () => void }> = ({ first, onRetry }) => {
  const { held, row, finished } = first;
  const left = useSecondsLeft(held?.liftsAt ?? null);
  if (held) return <StatusMark kind="changing" word={left === null ? 'queueing…' : waitingSentence(left)} />;
  if (row) {
    const transferring = row.phase === 'downloading' && row.percent !== undefined;
    if (!transferring) return <StatusMark kind="changing" word={row.text.status} />;
    return (
      <span className="flex flex-col gap-xs">
        <span
          role="progressbar"
          aria-label={`${row.text.title} downloaded`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={row.percent}
          className="h-[2px] w-full overflow-hidden rounded-full bg-surface-elevated"
        >
          {/* Dynamic: the bar's fill is the row's percentage. */}
          <span className="block h-full bg-text-secondary" style={{ width: `${row.percent}%` }} />
        </span>
        {/* A note (a reconnect, a part being checked) leads; the row then has no speed or time remaining. */}
        <span className="tabular-nums">{[row.text.notice, row.text.bytes, row.text.speed, row.text.eta].filter(Boolean).join(' · ')}</span>
      </span>
    );
  }
  if (finished === null) return null;
  if (finished.outcome.kind === 'completed') return <StatusMark kind="on" word={finished.text} />;
  return <Problem onRetry={onRetry}>{finished.text}</Problem>;
};

interface FirstModelStepProps {
  rec: RecommendationDto | null;
  /** Why nothing is suggested, when it is not that nothing fits, in the daemon's words. */
  noneReason: string | null;
  /** Whether the accelerator is being set up, which Get it waits for. */
  preparing: boolean;
  first: FirstModel;
  alreadyDone: boolean;
  /** Move on by itself; `null` once it has. */
  advance: (() => void) | null;
  onNext: () => void;
  onBack: () => void;
}

export const FirstModelStep: FC<FirstModelStepProps> = ({ rec, noneReason, first, preparing, alreadyDone, advance, onNext, onBack }) => {
  useAdvance(alreadyDone, advance);
  const get = () => void first.get();

  if (alreadyDone) {
    return (
      <Step n={3} title="First model" onBack={onBack} actions={<Button onClick={onNext}>Continue</Button>}>
        <StatusMark kind="on" word="Already done" />
      </Step>
    );
  }

  // Skip and Choose another move on at once; back here, the choice is open again.
  const leave = (choice: 'skip' | 'another') => {
    first.choose(choice);
    onNext();
  };
  const another = (
    <Button variant="ghost" size="sm" onClick={() => leave('another')}>
      Choose another
    </Button>
  );
  const skip = (
    <Button variant="ghost" size="sm" onClick={() => leave('skip')}>
      Skip
    </Button>
  );
  const offered = rec !== null;

  return (
    <Step
      n={3}
      title="First model"
      onBack={onBack}
      actions={
        first.choice === 'get' || first.choice === 'joined' ? (
          <Button onClick={onNext}>Continue</Button>
        ) : (
          <>
            {another}
            {skip}
            {offered && (
              <Button onClick={get} isLoading={first.queueing}>
                Get it
              </Button>
            )}
          </>
        )
      }
    >
      {rec === null && <p className="m-0 text-sm text-text-secondary">{noneReason ?? 'Nothing gglib would suggest fits.'}</p>}
      {rec !== null && (
        <Item name="Suggested" state={<code className="min-w-0 break-all font-mono text-text">{rec.repo}</code>}>
          <p className="m-0">{formatRecommendation(rec)}</p>
          {first.queueing && preparing && <StatusMark kind="changing" word="setting up faster downloads first" className="text-xs" />}
          {first.choice === 'get' && <Download first={first} onRetry={get} />}
          {first.queueError !== null && <Problem onRetry={get}>{first.queueError}</Problem>}
        </Item>
      )}
      {first.choice !== 'get' && <Join onJoined={() => first.choose('joined')} />}
    </Step>
  );
};
