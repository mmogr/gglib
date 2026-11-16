/**
 * The endpoint line: the signature of the sidebar, on every screen.
 *
 * A `PlaceRow`: the plug with the state's shape as its badge, `Endpoint`, and
 * the state word at the right (with a key glyph when a request must carry
 * the key); under the name, the address a client reaches it at with ⧉
 * beside it, then one secondary line: how many models are loaded, "nothing
 * loaded yet", the pinned model's name alone for a pinned run (the word
 * already says Pinned), or "click to turn on" (the word already says Off).
 * Every fact is the daemon's: the state word from `ENDPOINT_WORDS`, the
 * address from the status's `base_url` (the daemon's dial rule, a wildcard
 * bind reached on loopback), the key rule from `KEY_WORDS`, the pinned
 * model's name, the count from the server registry. The sentence around a
 * count is this line's own, until a place says it. ⧉ copies the whole
 * address without opening anything.
 *
 * The badge blinks once (150 ms) each time the proxy has served another
 * request (`components/status/useRequestBlink`), read from the one
 * dashboard stream (`useEndpointDashboard`, the endpoint place's). It is
 * the only motion at rest.
 *
 * Clicking the line opens the endpoint place (`/endpoint`); it is drawn
 * raised while that place is open.
 *
 * @module sidebar/EndpointLine
 */

import { useNavigate } from 'react-router';
import { KeyRound, Plug } from 'lucide-react';
import { paths } from '../app/paths';
import { CopyValue } from '../components/CopyValue';
import type { MarkKind } from '../components/status/StatusMark';
import { useRequestBlink } from '../components/status/useRequestBlink';
import { ENDPOINT_WORDS, KEY_WORDS } from '../components/status/words';
import { Icon } from '../components/ui/Icon';
import { useEndpointDashboard } from '../places/library/useResidency';
import { useEndpointFacts, useEndpointRefusal, type EndpointFacts } from '../services/proxyRegistry';
import { useAllServerStates } from '../services/serverRegistry';
import type { EndpointState } from '../types/generated/EndpointState';
import { PlaceRow } from './PlaceRow';

const MARK: Record<EndpointState, MarkKind> = {
  on: 'on',
  pinned: 'on',
  starting: 'changing',
  off: 'off',
};

/** The address as shown: the daemon's URL without its scheme. */
export function shownAddress(baseUrl: string): string {
  return baseUrl.replace(/^https?:\/\//, '');
}

/**
 * The second line, from the endpoint's facts and how many models are loaded;
 * the state word is said beside the name. An endpoint off because its port
 * was refused says so, and the page it opens says why in the daemon's words.
 */
export function secondLine(facts: EndpointFacts, loaded: number, refused: string | null = null): string {
  if (facts.state === 'off') return refused === null ? 'click to turn on' : 'did not start, click for why';
  if (facts.state === 'starting') return ENDPOINT_WORDS.starting;
  if (facts.pinned !== null) return facts.pinned.name;
  if (loaded === 0) return 'nothing loaded yet';
  return `${loaded} loaded`;
}

export function EndpointLine({ current = false }: { current?: boolean }) {
  const facts = useEndpointFacts();
  const refused = useEndpointRefusal();
  const blinking = useRequestBlink(useEndpointDashboard().snapshot?.total_requests ?? null);
  const loaded = useAllServerStates().filter((s) => s.status === 'running').length;
  const navigate = useNavigate();

  const word = ENDPOINT_WORDS[facts.state];
  const keyRequired = facts.key === 'required';
  const keyLabel = `Key ${KEY_WORDS.required.toLowerCase()}`;

  return (
    <PlaceRow
      name="Endpoint"
      icon={Plug}
      badge={{ kind: MARK[facts.state], label: word, blink: blinking }}
      word={
        <>
          <span>{word}</span>
          {keyRequired && (
            <span title={keyLabel} aria-label={keyLabel} role="img">
              <Icon icon={KeyRound} size={12} />
            </span>
          )}
        </>
      }
      after={
        <>
          {facts.baseUrl !== null && (
            <div className="flex items-center justify-between gap-xs">
              <span className="font-mono text-xs text-text-secondary truncate" title={facts.baseUrl}>
                {shownAddress(facts.baseUrl)}
              </span>
              <CopyValue value={facts.baseUrl} label="Copy the endpoint address" />
            </div>
          )}
          <span title={refused ?? undefined}>{secondLine(facts, loaded, refused)}</span>
        </>
      }
      current={current}
      onClick={() => navigate(paths.endpoint())}
      label={`Endpoint, ${word}`}
    />
  );
}
