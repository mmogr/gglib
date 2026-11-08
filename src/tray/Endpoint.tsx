/**
 * The tray's Endpoint block: `Endpoint ● On`, the address a client reaches
 * it at with ⧉, and the lines that are true of this run.
 *
 * Every fact is the daemon's (`proxyRegistry`'s `EndpointFacts`): the state
 * word from `ENDPOINT_WORDS`, the address from the status's `base_url`, the
 * key rule from `KEY_WORDS`, the pinned model's name. Pinned reads
 * "On · only <model>". While a turn-on is on its way, or the endpoint is up
 * but its status has not yet said where (Connecting), the mark turns (◌).
 * The ● blinks once per served request, as the sidebar's does
 * (`useRequestBlink`). Key required puts `▲ Key required · Copy key` under the address; Copy key
 * copies the key the panel already revealed for its dashboard stream, so it
 * waits for that reveal. Off dims the bar to "Off. Clients can't connect.";
 * Turn on is the footer's. A copy says "Copied" for one second beside its
 * button, since the popover has no toast host; a copy that fails says why.
 *
 * @module tray/Endpoint
 */

import { useEffect, useState, type FC } from 'react';
import { CopyValue } from '../components/CopyValue';
import { StatusMark, type MarkKind } from '../components/status/StatusMark';
import { useRequestBlink } from '../components/status/useRequestBlink';
import { ENDPOINT_WORDS, KEY_WORDS } from '../components/status/words';
import { Button } from '../components/ui/Button';
import type { EndpointFacts } from '../services/proxyRegistry';
import type { EndpointBusy } from '../places/endpoint/useEndpointActions';
import { formatError } from '../utils/errors';
import { Failure } from './Failure';

/** How long "Copied" stays up, in ms. */
export const COPIED_MS = 1000;

/** `[shown, show]`: `shown` turns true on `show()` and back after `COPIED_MS`. */
function useBriefly(): [boolean, () => void] {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!shown) return undefined;
    const timer = setTimeout(() => setShown(false), COPIED_MS);
    return () => clearTimeout(timer);
  }, [shown]);
  return [shown, () => setShown(true)];
}

const Copied: FC<{ shown: boolean }> = ({ shown }) =>
  shown ? (
    <span role="status" className="shrink-0 text-xs text-text-secondary">
      Copied
    </span>
  ) : null;

/** What the endpoint line says, as a mark and a word. */
export function endpointSaying(facts: EndpointFacts, busy: EndpointBusy): { mark: MarkKind; word: string } {
  if (busy === 'on' || facts.state === 'starting') return { mark: 'changing', word: ENDPOINT_WORDS.starting };
  if (facts.state === 'off') return { mark: 'off', word: ENDPOINT_WORDS.off };
  // Up, but the status has not said where yet: Connecting.
  if (facts.baseUrl === null) return { mark: 'changing', word: ENDPOINT_WORDS.on };
  if (facts.pinnedModel !== null) return { mark: 'on', word: `${ENDPOINT_WORDS.on} · only ${facts.pinnedModel}` };
  return { mark: 'on', word: ENDPOINT_WORDS.on };
}

interface EndpointProps {
  facts: EndpointFacts;
  busy: EndpointBusy;
  /** The daemon's words for the last turn on or off that failed. */
  error: string | null;
  /** The proxy's key: `undefined` until revealed, `null` when none could be read. */
  apiKey: string | null | undefined;
  /** The proxy's request count from the dashboard stream, `null` while it is idle: the dot blinks as it grows. */
  served: number | null;
}

export const Endpoint: FC<EndpointProps> = ({ facts, busy, error, apiKey, served }) => {
  const blinking = useRequestBlink(served);
  const [urlCopied, showUrlCopied] = useBriefly();
  const [keyCopied, showKeyCopied] = useBriefly();
  const saying = endpointSaying(facts, busy);
  const up = facts.state !== 'off';

  const [copyError, setCopyError] = useState<string | null>(null);
  const copyKey = () => {
    if (!apiKey) return;
    setCopyError(null);
    navigator.clipboard
      .writeText(apiKey)
      .then(showKeyCopied)
      .catch((err: unknown) => setCopyError(formatError(err)));
  };

  return (
    <section aria-label="Endpoint" className="flex flex-col gap-xs pb-md">
      <div className="flex items-center justify-between px-md">
        <span className="text-sm text-text">Endpoint</span>
        <StatusMark kind={saying.mark} word={saying.word} blink={blinking} className="min-w-0 truncate" />
      </div>

      <div className="mx-md flex min-h-[36px] items-center gap-xs rounded-base bg-surface-elevated px-sm">
        {up && facts.baseUrl !== null ? (
          <>
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-text" title={facts.baseUrl}>
              {facts.baseUrl}
            </span>
            <Copied shown={urlCopied} />
            <CopyValue value={facts.baseUrl} label="Copy the endpoint address" onCopied={showUrlCopied} />
          </>
        ) : (
          <span className="flex-1 text-xs text-text-muted">
            {up ? '' : "Off. Clients can't connect."}
          </span>
        )}
      </div>

      {up && facts.key === 'required' && (
        <p className="m-0 flex items-center gap-xs px-md text-xs text-text-secondary">
          <StatusMark kind="attention" word={`Key ${KEY_WORDS.required.toLowerCase()}`} className="text-xs" />
          <span aria-hidden>·</span>
          <Button variant="link" size="sm" className="text-xs font-normal" disabled={!apiKey} onClick={copyKey}>
            Copy key
          </Button>
          <Copied shown={keyCopied} />
        </p>
      )}

      {copyError !== null && <Failure message={copyError} />}
      {error !== null && <Failure message={error} />}
    </section>
  );
};
