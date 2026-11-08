/**
 * A state as a shape and a word, never a colour alone.
 *
 * The one way a state is drawn, from the visual system's table: a filled
 * dot for a thing that is up, a hollow dot for one that is off, an arc that
 * turns only while something is changing, an amber hollow dot for a machine
 * that has gone quiet, a triangle for a line that wants attention, a cross
 * for a failure. The word beside it is the caller's, from `words.ts` when
 * the daemon has one; the shape says the same thing to a reader who cannot
 * see the colour.
 *
 * @module components/status/StatusMark
 */

import type { FC } from 'react';
import { Triangle, X } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';
import type { ReachState } from '../../types/generated/ReachState';
import type { ResidencyState } from '../../types/generated/ResidencyState';

/** The kinds of mark, one per row of the visual system's status table. */
export type MarkKind = 'on' | 'off' | 'changing' | 'away' | 'attention' | 'failed';

/** The shape for each reach, beside `REACH_WORDS`. */
export const REACH_MARK: Readonly<Record<ReachState, MarkKind>> = {
  connected: 'on',
  away: 'away',
  disconnected: 'off',
};

/** The shape for each residency, beside `RESIDENCY_WORDS`: only a load in progress turns. */
export const RESIDENCY_MARK: Readonly<Record<ResidencyState, MarkKind>> = {
  loaded: 'on',
  loading: 'changing',
  working: 'on',
  reading: 'on',
  waiting: 'on',
  not_loaded: 'off',
};

interface StatusMarkProps {
  kind: MarkKind;
  /** The state's word, drawn after the shape; omitted, the shape stands alone with `label` for readers. */
  word?: string;
  /** What the shape says when it stands alone (`aria-label`); the word when one is drawn. */
  label?: string;
  /** The `on` dot's once-per-request blink (`useRequestBlink`), the endpoint's alone; no other shape blinks. */
  blink?: boolean;
  className?: string;
}

const DOT = 'inline-block w-2 h-2 rounded-full shrink-0';

/** The shape for each kind: a glyph or a styled span, each the same footprint. */
function Shape({ kind, blink }: { kind: MarkKind; blink: boolean }) {
  switch (kind) {
    case 'on':
      return <span aria-hidden className={cn(DOT, 'bg-success', blink && 'animate-request-blink')} />;
    case 'off':
      return <span aria-hidden className={cn(DOT, 'border border-text-muted')} />;
    case 'changing':
      return (
        <span
          aria-hidden
          className={cn(DOT, 'border border-text-secondary border-t-transparent animate-spin-360')}
        />
      );
    case 'away':
      return <span aria-hidden className={cn(DOT, 'border border-warning')} />;
    case 'attention':
      return (
        <span aria-hidden className="inline-flex shrink-0 text-warning">
          <Icon icon={Triangle} size={10} strokeWidth={2.2} />
        </span>
      );
    case 'failed':
      return (
        <span aria-hidden className="inline-flex shrink-0 text-danger">
          <Icon icon={X} size={10} strokeWidth={2.2} />
        </span>
      );
  }
}

export const StatusMark: FC<StatusMarkProps> = ({ kind, word, label, blink = false, className }) => (
  <span
    className={cn('inline-flex items-center gap-xs text-sm text-text-secondary tabular-nums', className)}
    aria-label={word === undefined ? label : undefined}
    role={word === undefined && label !== undefined ? 'img' : undefined}
    data-kind={kind}
  >
    <Shape kind={kind} blink={blink} />
    {word !== undefined && <span>{word}</span>}
  </span>
);
