/**
 * The sheet's Speculative field, for a model with MTP heads: drafting on or
 * off, the draft tokens and the minimum probability, each empty field
 * showing the plan's value (its `mtp` arguments) and whose choice it was;
 * Reset puts all three back to the plan's.
 *
 * @module inspector/Speculative
 */

import type { FC, Ref } from 'react';
import { Button } from '../components/ui/Button';
import { Checkbox } from '../components/ui/Checkbox';
import { Input } from '../components/ui/Input';
import type { LaunchArg } from '../types/generated/LaunchArg';
import { planValue, type Typed } from './loadOptions';
import { argsWhy } from './local/rows';

interface SpeculativeProps {
  typed: Pick<Typed, 'mtp' | 'nMax' | 'pMin'>;
  /** The plan's `mtp` arguments: drafting is on without the sheet when there are any. */
  args: LaunchArg[];
  disabled: boolean;
  onChange: (change: Partial<Typed>) => void;
  focusRef?: Ref<HTMLInputElement>;
}

export const Speculative: FC<SpeculativeProps> = ({ typed, args, disabled, onChange, focusRef }) => {
  const on = typed.mtp ?? args.length > 0;
  const touched = typed.mtp !== null || typed.nMax !== '' || typed.pMin !== '';

  return (
    <fieldset aria-label="Speculative" className="m-0 flex flex-col gap-xs border-0 p-0">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold text-text">Speculative</span>
        {touched && (
          <Button variant="link" size="sm" className="h-auto p-0 text-xs font-normal" onClick={() => onChange({ mtp: null, nMax: '', pMin: '' })}>
            Reset
          </Button>
        )}
      </div>
      <Checkbox ref={focusRef} label="MTP" checked={on} disabled={disabled} onChange={(e) => onChange({ mtp: e.target.checked })} />
      {args.length > 0 && <span className="text-xs text-text-secondary">{argsWhy(args)}</span>}
      {on && (
        <div className="flex gap-sm">
          <label className="flex flex-1 flex-col gap-xs text-xs text-text-secondary">
            Draft tokens
            <Input
              size="sm"
              type="number"
              min={1}
              value={typed.nMax}
              placeholder={planValue(args, '--spec-draft-n-max')}
              disabled={disabled}
              onChange={(e) => onChange({ nMax: e.target.value })}
            />
          </label>
          <label className="flex flex-1 flex-col gap-xs text-xs text-text-secondary">
            Min probability
            <Input
              size="sm"
              type="number"
              min={0}
              max={1}
              step={0.05}
              value={typed.pMin}
              placeholder={planValue(args, '--spec-draft-p-min')}
              disabled={disabled}
              onChange={(e) => onChange({ pMin: e.target.value })}
            />
          </label>
        </div>
      )}
    </fieldset>
  );
};
