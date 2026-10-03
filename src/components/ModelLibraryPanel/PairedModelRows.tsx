import { FC } from 'react';
import { Server } from 'lucide-react';
import type { PairedModelsState, PairedReach } from '../../hooks/usePairedModels';
import type { ModelInfo } from '../../types/generated/ModelInfo';
import type { ModelRef } from '../../types/generated/ModelRef';
import { Icon } from '../ui/Icon';
import { Chip, type ChipVariant } from '../ui/Chip';
import { VisionChip } from '../VisionChip';
import { canSee } from '../../utils/canSee';
import { cn } from '../../utils/cn';

interface PairedModelRowsProps {
  paired: PairedModelsState;
  /** The library's search text, matched as this machine's rows match it. */
  searchQuery: string;
  /** Whether a filter is on. Filters describe this machine's models, so none applies here. */
  filtersActive: boolean;
  /** The far model picked, if one is. */
  picked: ModelRef | null;
  onPick: (model: ModelRef) => void;
}

const REACH: Record<PairedReach, { label: string; variant: ChipVariant; title: string }> = {
  reached: { label: 'reached', variant: 'success', title: 'These are its models now' },
  away: { label: 'away', variant: 'warning', title: 'It is away; these are its models when it was last reached' },
  stale: { label: 'stale', variant: 'warning', title: 'Its models could not be read; these are the last ones read' },
};

function matches(model: ModelInfo, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return model.id.toLowerCase().includes(q) || (model.description?.toLowerCase().includes(q) ?? false);
}

/**
 * The paired machine's models, after this machine's: a group headed by
 * that machine's name and whether it is reached, then one row per model,
 * each badged with the machine and, when it reads images, with the same
 * Vision chip this machine's rows carry. A row is read-only here; picking
 * it opens the far inspector, which offers what may be done there.
 *
 * Rows are keyed by the machine's fingerprint and the model's id there, so
 * the same model on both machines is two rows that cannot be confused.
 * Nothing shows until that machine's models have been read once.
 */
export const PairedModelRows: FC<PairedModelRowsProps> = ({ paired, searchQuery, filtersActive, picked, onPick }) => {
  const { group, name, reach } = paired;
  if (group === null || group.machine.kind !== 'paired') return null;
  const machine = group.machine;
  const shown = group.models.filter((m) => matches(m, searchQuery));
  const badge = REACH[reach];
  return (
    <section aria-label={`${name}'s models`} className="border-t border-border-light">
      <div className="flex items-center gap-sm py-sm px-md text-xs font-semibold text-text-secondary">
        <Icon icon={Server} size={13} />
        <span className="min-w-0 break-words">{name}</span>
        <Chip size="sm" variant={badge.variant} title={badge.title}>{badge.label}</Chip>
      </div>
      {filtersActive ? (
        <p className="m-0 py-sm px-md text-xs text-text-muted">
          {`Filters describe this machine's models; clear them to see ${name}'s.`}
        </p>
      ) : shown.length === 0 ? (
        <p className="m-0 py-sm px-md text-xs text-text-muted">
          {group.models.length === 0 ? `${name} has no models.` : `None of ${name}'s models match.`}
        </p>
      ) : (
        <div className="flex flex-col w-full" role="listbox" aria-label={`${name}'s models`}>
          {shown.map((model) => {
            const isSelected =
              picked?.id === model.gglib_id &&
              picked.machine.kind === 'paired' &&
              picked.machine.fingerprint === machine.fingerprint;
            return (
              // eslint-disable-next-line no-restricted-syntax -- role="option" listbox row, not a Button-shaped control
              <button
                key={`${machine.fingerprint}:${model.gglib_id}`}
                type="button"
                role="option"
                aria-selected={isSelected}
                className={cn(
                  'py-sm px-md text-left border-l-[3px] border-l-transparent cursor-pointer transition duration-200 w-full bg-transparent hover:bg-background-hover focus-visible:outline-none focus-visible:bg-background-hover focus-visible:border-l-primary',
                  isSelected && 'bg-primary-subtle border-l-primary',
                )}
                onClick={() => onPick({ machine, id: model.gglib_id })}
              >
                <div className="flex flex-col gap-xs w-full">
                  <div className="font-medium text-sm flex items-center gap-sm w-full break-words">
                    {model.id}
                    <Chip size="sm" leftIcon={<Icon icon={Server} size={11} />}>{name}</Chip>
                  </div>
                  <div className="flex items-center gap-md text-xs text-text-muted flex-wrap">
                    {model.description && <span className="inline-flex items-center">{model.description}</span>}
                    {canSee(model) && <VisionChip />}
                    {model.context_window != null && (
                      <Chip size="sm" className="font-mono tabular-nums">
                        {model.context_window.toLocaleString()} ctx
                      </Chip>
                    )}
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
};
