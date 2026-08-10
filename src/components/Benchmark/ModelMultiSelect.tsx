import type { FC } from 'react';
import { Checkbox } from '../ui/Checkbox';
import { cn } from '../../utils/cn';
import type { GgufModel } from '../../types';

interface ModelMultiSelectProps {
  models: GgufModel[];
  selectedIds: number[];
  onToggle: (id: number) => void;
  disabled?: boolean;
}

/** The benchmark config panel's model checkbox list. */
export const ModelMultiSelect: FC<ModelMultiSelectProps> = ({
  models,
  selectedIds,
  onToggle,
  disabled = false,
}) => (
  <div className="flex flex-col gap-sm">
    <label className="text-xs font-semibold text-text">
      Models ({selectedIds.length} selected)
    </label>
    <div className="flex flex-col gap-xs max-h-[240px] overflow-y-auto border border-border rounded-md p-xs">
      {models.length === 0 && <p className="text-xs text-text-muted p-xs">No models available</p>}
      {models.map((m) => {
        if (m.id == null) return null;
        const checked = selectedIds.includes(m.id);
        return (
          <Checkbox
            key={m.id}
            checked={checked}
            disabled={disabled}
            onChange={() => onToggle(m.id!)}
            wrapperClassName={cn(
              'w-full p-xs rounded-sm transition-colors',
              checked ? 'bg-primary-subtle' : 'hover:bg-surface-elevated',
            )}
            label={
              <span className={cn('block truncate', checked ? 'text-text' : 'text-text-secondary')}>
                {m.name}
              </span>
            }
          />
        );
      })}
    </div>
  </div>
);
