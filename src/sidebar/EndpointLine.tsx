/**
 * The endpoint line: the signature of the sidebar, on every screen.
 *
 * `● Endpoint  On`, the address a client reaches it at with ⧉ beside it,
 * and one secondary line: how many models are loaded, "nothing loaded yet",
 * "only <model>" for a pinned run, or "Off · click to turn on". Every fact
 * is the daemon's: the state word from `ENDPOINT_WORDS`, the address from
 * the status's `base_url` (the daemon's dial rule, a wildcard bind reached
 * on loopback), the key rule from `KEY_WORDS`, the pinned model's name, the
 * count from the server registry. The sentence around a count is this
 * line's own, until a place says it. ⧉ copies the whole address without
 * opening anything.
 *
 * The dot blinks once (150 ms) each time the proxy has served another
 * request: its `total_requests` grew between two frames of the one
 * dashboard stream (`useEndpointDashboard`, the endpoint place's). At most
 * once a second, never for the first frame or a restarted proxy's count,
 * and not at all under reduced motion. It is the only motion at rest.
 *
 * Clicking the line opens the endpoint place (`/endpoint`); it is drawn
 * raised while that place is open.
 *
 * @module sidebar/EndpointLine
 */

import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router';
import { KeyRound } from 'lucide-react';
import { paths } from '../app/paths';
import { CopyValue } from '../components/CopyValue';
import { StatusMark, type MarkKind } from '../components/status/StatusMark';
import { ENDPOINT_WORDS, KEY_WORDS } from '../components/status/words';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';
import { useToastContext } from '../contexts/ToastContext';
import { useEndpointDashboard } from '../places/library/useResidency';
import { useEndpointFacts, type EndpointFacts } from '../services/proxyRegistry';
import { useAllServerStates } from '../services/serverRegistry';
import type { EndpointState } from '../types/generated/EndpointState';
import { cn } from '../utils/cn';
import { SPREAD } from './placement';

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

/** The second line, from the endpoint's facts and how many models are loaded. */
export function secondLine(facts: EndpointFacts, loaded: number): string {
  if (facts.state === 'off') return `${ENDPOINT_WORDS.off} · click to turn on`;
  if (facts.state === 'starting') return ENDPOINT_WORDS.starting;
  if (facts.pinnedModel !== null) return `only ${facts.pinnedModel}`;
  if (loaded === 0) return 'nothing loaded yet';
  return `${loaded} loaded`;
}

/** How long the dot's blink lasts. */
export const BLINK_MS = 150;
/** The least time between two blinks. */
const BLINK_GAP_MS = 1000;

/**
 * Whether a frame counting `served` requests blinks the dot: only when the
 * count grew since the frame before (`seen`, `null` for none), and not
 * within a second of the last blink (`blinkedAt`).
 */
export function shouldBlink(seen: number | null, served: number, blinkedAt: number | null, now: number): boolean {
  if (seen === null || served <= seen) return false;
  return blinkedAt === null || now - blinkedAt >= BLINK_GAP_MS;
}

function reducedMotion(): boolean {
  return typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Whether the dot is blinking now, for the proxy's request count (`null` while its stream is idle). */
function useRequestBlink(served: number | null): boolean {
  const [blinking, setBlinking] = useState(false);
  const seen = useRef<number | null>(null);
  const blinkedAt = useRef<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  useEffect(() => {
    const before = seen.current;
    seen.current = served;
    const now = Date.now();
    if (served === null || !shouldBlink(before, served, blinkedAt.current, now) || reducedMotion()) return;
    blinkedAt.current = now;
    setBlinking(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setBlinking(false), BLINK_MS);
  }, [served]);
  return blinking;
}

export function EndpointLine({ current = false }: { current?: boolean }) {
  const facts = useEndpointFacts();
  const blinking = useRequestBlink(useEndpointDashboard().snapshot?.total_requests ?? null);
  const loaded = useAllServerStates().filter((s) => s.status === 'running').length;
  const navigate = useNavigate();
  const { showToast } = useToastContext();

  const word = ENDPOINT_WORDS[facts.state];
  const keyRequired = facts.key === 'required';
  const keyLabel = `Key ${KEY_WORDS.required.toLowerCase()}`;

  return (
    <div className="px-sm">
      <div className={cn('flex flex-col gap-xs rounded-base px-sm py-sm', current && 'bg-surface-hover')}>
        <Button
          variant="ghost"
          fullWidth
          className={cn('h-auto gap-sm px-0 py-0', SPREAD)}
          onClick={() => navigate(paths.endpoint())}
          aria-current={current ? 'page' : undefined}
          aria-label={`Endpoint, ${word}`}
        >
          <span className="flex items-center gap-sm text-sm text-text">
            <StatusMark kind={MARK[facts.state]} label={word} className={blinking ? 'animate-request-blink' : undefined} />
            <span>Endpoint</span>
          </span>
          <span className="flex items-center gap-xs text-sm text-text-secondary">
            <span>{word}</span>
            {keyRequired && (
              <span title={keyLabel} aria-label={keyLabel} role="img">
                <Icon icon={KeyRound} size={12} />
              </span>
            )}
          </span>
        </Button>
        {facts.baseUrl !== null && (
          <div className="flex items-center justify-between gap-xs">
            <span className="font-mono text-xs text-text-secondary truncate" title={facts.baseUrl}>
              {shownAddress(facts.baseUrl)}
            </span>
            <CopyValue
              value={facts.baseUrl}
              label="Copy the endpoint address"
              onCopied={() => showToast('Endpoint address copied', 'success')}
            />
          </div>
        )}
        <span className="text-xs text-text-muted">{secondLine(facts, loaded)}</span>
      </div>
    </div>
  );
}
