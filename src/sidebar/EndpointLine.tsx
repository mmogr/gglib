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
 * Clicking the line opens today's proxy panel beside it (Turn on / off, the
 * dashboard, Clear Cache), until the Endpoint place takes the click (N1).
 *
 * @module sidebar/EndpointLine
 */

import { useRef, useState } from 'react';
import { KeyRound } from 'lucide-react';
import { CopyValue } from '../components/CopyValue';
import { ProxyPopover } from '../components/ProxyControl';
import { StatusMark, type MarkKind } from '../components/status/StatusMark';
import { ENDPOINT_WORDS, KEY_WORDS } from '../components/status/words';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';
import { useToastContext } from '../contexts/ToastContext';
import { useClickOutside } from '../hooks/useClickOutside';
import { useEndpointFacts, type EndpointFacts } from '../services/proxyRegistry';
import { useAllServerStates } from '../services/serverRegistry';
import type { EndpointState } from '../types/generated/EndpointState';
import { POPOVER_BESIDE } from './placement';

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

export function EndpointLine() {
  const facts = useEndpointFacts();
  const loaded = useAllServerStates().filter((s) => s.status === 'running').length;
  const [open, setOpen] = useState(false);
  const lineRef = useRef<HTMLDivElement>(null);
  const { showToast } = useToastContext();

  useClickOutside(lineRef, () => setOpen(false), open);

  const word = ENDPOINT_WORDS[facts.state];
  const keyRequired = facts.key === 'required';
  const keyLabel = `Key ${KEY_WORDS.required.toLowerCase()}`;

  return (
    <div className="relative px-sm" ref={lineRef}>
      <div className="flex flex-col gap-xs px-sm py-sm">
        <Button
          variant="ghost"
          fullWidth
          className="h-auto justify-between gap-sm px-0 py-0"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
          aria-label={`Endpoint, ${word}`}
        >
          <span className="flex items-center gap-sm text-sm text-text">
            <StatusMark kind={MARK[facts.state]} label={word} />
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

      <ProxyPopover open={open} onClose={() => setOpen(false)} className={POPOVER_BESIDE} />
    </div>
  );
}
