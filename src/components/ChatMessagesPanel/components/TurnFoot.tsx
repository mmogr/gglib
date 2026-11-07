import { FC, Fragment, ReactNode, useState, type FocusEvent } from 'react';
import { UNFINISHED, figureDetail, figureParts, turnTime, type ReplyFacts } from '../../../readings/turnFigures';
import { cn } from '../../../utils/cn';

/**
 * Whether a turn shows what it shows only on hover: while the pointer is
 * over it, or keyboard focus is in it. Held in React rather than in CSS so
 * that focus reaching an action reveals the line it is on.
 */
export function useTurnReveal() {
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  return {
    shown: hovered || focused,
    handlers: {
      onMouseEnter: () => setHovered(true),
      onMouseLeave: () => setHovered(false),
      onFocus: () => setFocused(true),
      onBlur: (event: FocusEvent<HTMLElement>) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false);
      },
    },
  };
}

/**
 * A reply's figure line, one 12 px line, with every figure it has in its
 * tooltip; nothing when it has none. "unfinished" is in the warning colour:
 * the reply is degraded (SY-1).
 */
export const FigureLine: FC<{ facts: ReplyFacts }> = ({ facts }) => {
  const parts = figureParts(facts);
  if (parts.length === 0) return null;
  return (
    <span className="tabular-nums" title={figureDetail(facts).join('\n')}>
      {parts.map((part, at) => (
        <Fragment key={part}>
          {at > 0 && ' · '}
          {part === UNFINISHED ? <span className="text-warning">{part}</span> : part}
        </Fragment>
      ))}
    </span>
  );
};

interface TurnFootProps {
  /** Shown on hover only: when the turn was sent or saved. */
  at?: Date;
  /** The turn is hovered or holds focus. */
  shown: boolean;
  /** A reply's figure line, and whether it is always shown (the latest reply's). */
  figures?: ReactNode;
  figuresAlways?: boolean;
  /** The action line, and whether it is always shown. */
  actions?: ReactNode;
  actionsAlways?: boolean;
  /** The switcher of a branch point this turn starts: always shown. */
  branches?: ReactNode;
  align?: 'start' | 'end';
}

/**
 * The line under a turn: its time on hover, a reply's figures, its action
 * line and, where the chat's branches part at it, `‹ 2/3 ›`. The action line
 * is always in the page, only faded, so that a keyboard reaches it; the
 * line keeps its height, so nothing moves when it shows.
 */
export const TurnFoot: FC<TurnFootProps> = ({
  at,
  shown,
  figures,
  figuresAlways = false,
  actions,
  actionsAlways = false,
  branches,
  align = 'start',
}) => (
  <div
    className={cn(
      'flex flex-wrap items-center gap-x-md gap-y-xs min-h-[24px] text-xs text-text-muted',
      align === 'end' && 'justify-end',
    )}
  >
    {shown && at && (
      <time className="tabular-nums" dateTime={at.toISOString()}>
        {turnTime(at)}
      </time>
    )}
    {(shown || figuresAlways) && figures}
    {actions && (
      <div
        data-shown={shown || actionsAlways}
        className="flex items-center gap-sm transition-opacity duration-150 data-[shown=false]:opacity-0"
      >
        {actions}
      </div>
    )}
    {branches}
  </div>
);
