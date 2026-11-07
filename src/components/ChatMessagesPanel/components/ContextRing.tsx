import { FC, useId, useRef, useState } from 'react';
import { Button } from '../../ui/Button';
import { useClickOutside } from '../../../hooks/useClickOutside';
import { cn } from '../../../utils/cn';
import { useContextReading } from '../hooks/useContextReading';
import {
  contextLines,
  contextTooltip,
  percentText,
  spokenReading,
  type ContextReading,
} from '../../../readings/contextReading';

const SIZE = 14;
const STROKE = 2;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/** The ring itself, `◔`: an arc for the share used, in the text's own colour. */
const Ring: FC<{ percent: number }> = ({ percent }) => (
  <svg width={SIZE} height={SIZE} viewBox={`0 0 ${SIZE} ${SIZE}`} className="-rotate-90" aria-hidden>
    <circle cx={SIZE / 2} cy={SIZE / 2} r={RADIUS} fill="none" strokeWidth={STROKE} className="stroke-border" />
    <circle
      cx={SIZE / 2}
      cy={SIZE / 2}
      r={RADIUS}
      fill="none"
      strokeWidth={STROKE}
      stroke="currentColor"
      strokeDasharray={CIRCUMFERENCE}
      strokeDashoffset={CIRCUMFERENCE * (1 - percent / 100)}
    />
  </svg>
);

/**
 * A reading's ring and its detail. Hovering says how much is used; a click
 * opens the detail, which Escape or a click elsewhere closes. Under 70% the
 * ring is alone; from 70% its figure stands beside it, so how full it is
 * never rests on colour.
 *
 * The detail is a `group`, not a `dialog`: `useClickOutside` counts any
 * dialog as inside every popout, so a dialog here would keep a menu open
 * under it.
 */
const ReadingRing: FC<{ reading: ContextReading }> = ({ reading }) => {
  const rootRef = useRef<HTMLDivElement>(null);
  const detailId = useId();
  const [open, setOpen] = useState(false);
  useClickOutside(rootRef, () => setOpen(false), open);
  const lines = contextLines(reading);
  const near = reading.severity !== 'normal';

  return (
    <div
      className="relative inline-flex"
      ref={rootRef}
      onKeyDown={(event) => {
        if (open && event.key === 'Escape') setOpen(false);
      }}
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className={cn(
          'gap-xs px-1.5 font-normal tabular-nums',
          reading.severity === 'danger' ? 'text-danger' : near ? 'text-warning' : 'text-text-secondary',
        )}
        aria-label={`Context: ${spokenReading(reading)}`}
        title={contextTooltip(reading)}
        aria-expanded={open}
        aria-controls={open ? detailId : undefined}
        onClick={() => setOpen(!open)}
      >
        <Ring percent={reading.percent} />
        {near && <span>{percentText(reading)}</span>}
      </Button>

      {open && (
        <div
          id={detailId}
          role="group"
          aria-label="Context"
          className="absolute bottom-full right-0 z-popover mb-1 flex w-max max-w-[240px] flex-col gap-xs rounded-lg bg-surface-elevated px-[14px] py-[10px] text-left text-xs text-text-secondary shadow-lg tabular-nums"
        >
          {lines.map((line, index) => (
            <p key={line} className={cn('m-0', index === 0 && 'text-text')}>
              {line}
            </p>
          ))}
        </div>
      )}
    </div>
  );
};

/**
 * The composer's context ring: how much of the model's context the
 * conversation has used, as its last finished reply's figures say.
 *
 * Nothing is drawn without a reading: no empty ring, no zero and no dash.
 * A detail left open goes with its reading.
 */
export const ContextRing: FC = () => {
  const reading = useContextReading();
  return reading ? <ReadingRing reading={reading} /> : null;
};
