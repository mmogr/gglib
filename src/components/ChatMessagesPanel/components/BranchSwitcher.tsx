import { FC, useContext, useId, useRef, useState } from 'react';
import { useMessage } from '@assistant-ui/react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { IconButton } from '../../ui/IconButton';
import { useClickOutside } from '../../../hooks/useClickOutside';
import { savedRowId } from '../../../hooks/useGglibRuntime/savedRows';
import type { GglibMessage } from '../../../types/messages';
import type { BranchPoint } from '../../../types/generated/BranchPoint';
import { cn } from '../../../utils/cn';
import { BranchingContext } from './BranchingContext';
import { TurnRow } from './TurnRow';
import { TurnWho } from './TurnMargin';

/** How an option is listed: its line, or what stands in for one. */
function optionLine(option: BranchPoint['options'][number]): string {
  if (option.message_id === null) return 'Nothing here yet';
  return option.preview || '(no text)';
}

/**
 * The options at a branch point, each a chat of the family: the previous
 * and the next open that chat, and the count opens the list of them, each
 * by its line, the open chat's marked. Escape or a click elsewhere closes
 * the list. The list is a `group`, as the context ring's detail is, so a
 * click in it is not a click outside a popout around it.
 */
export const BranchSwitcher: FC<{ point: BranchPoint; open: (cid: number) => void }> = ({ point, open }) => {
  const rootRef = useRef<HTMLDivElement>(null);
  const listId = useId();
  const [listOpen, setListOpen] = useState(false);
  useClickOutside(rootRef, () => setListOpen(false), listOpen);
  const { index, options } = point;
  const choose = (at: number) => {
    setListOpen(false);
    if (at !== index) open(options[at].conversation_id);
  };

  return (
    <div
      ref={rootRef}
      className="relative inline-flex items-center"
      onKeyDown={(event) => event.key === 'Escape' && setListOpen(false)}
    >
      <IconButton label="Previous branch" size="sm" disabled={index === 0} onClick={() => choose(index - 1)}>
        <Icon icon={ChevronLeft} size={14} />
      </IconButton>
      <Button
        variant="ghost"
        size="sm"
        className="px-1 font-mono tabular-nums"
        aria-label={`Branch ${index + 1} of ${options.length}`}
        aria-expanded={listOpen}
        aria-controls={listOpen ? listId : undefined}
        onClick={() => setListOpen(!listOpen)}
      >
        {index + 1}/{options.length}
      </Button>
      <IconButton label="Next branch" size="sm" disabled={index === options.length - 1} onClick={() => choose(index + 1)}>
        <Icon icon={ChevronRight} size={14} />
      </IconButton>

      {listOpen && (
        <div
          id={listId}
          role="group"
          aria-label="Branches here"
          className="absolute right-0 top-full mt-1 z-popover w-max max-w-[320px] flex flex-col py-xs bg-surface-elevated border border-border rounded-lg shadow-lg text-left text-sm"
        >
          {options.map((option, at) => (
            <Button
              key={option.conversation_id}
              variant="ghost"
              size="sm"
              aria-current={at === index || undefined}
              onClick={() => choose(at)}
              className={cn(
                'justify-start rounded-none truncate',
                at === index ? 'text-text font-medium' : 'text-text-secondary',
                option.message_id === null && 'italic',
              )}
            >
              {optionLine(option)}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
};

/** The switcher of the branch point a turn starts, in its margin; nothing where it starts none. */
export const TurnBranches: FC = () => {
  const branching = useContext(BranchingContext);
  const message = useMessage();
  const id = savedRowId(message as unknown as GglibMessage);
  const point = id === null ? undefined : branching?.points.find((p) => p.message_id === id);
  return point && branching ? <BranchSwitcher point={point} open={branching.open} /> : null;
};

/**
 * The row after a chat's last message where other branches of its family
 * go on: its switcher, the open chat's option being "Nothing here yet".
 */
export const BranchEnd: FC = () => {
  const branching = useContext(BranchingContext);
  const point = branching?.points.find((p) => p.message_id === null);
  if (!point || !branching) return null;
  return (
    <TurnRow
      who={<TurnWho name="Other branches" />}
      body={
        <div className="flex flex-wrap items-center gap-md">
          <p className="m-0 text-text-muted">Other branches go on from here.</p>
          <BranchSwitcher point={point} open={branching.open} />
        </div>
      }
    />
  );
};
