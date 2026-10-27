/**
 * The one input a secret is typed into: masked, with an eye beside it that
 * shows what was typed (ADR 0027).
 *
 * The eye is a real button next to the input, not one of `Input`'s icon
 * slots, which ignore the pointer.
 *
 * @module components/ui/SecretInput
 */

import { FC, useState } from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { cn } from '../../utils/cn';
import { Icon } from './Icon';
import { IconButton } from './IconButton';
import { Input, type InputProps } from './Input';

export interface SecretInputProps extends Omit<InputProps, 'type' | 'value' | 'onChange' | 'aria-label'> {
  value: string;
  onChange: (value: string) => void;
  /** What is typed here, for the input's and the eye's accessible names. */
  label: string;
}

export const SecretInput: FC<SecretInputProps> = ({ value, onChange, label, disabled, className, ...input }) => {
  const [visible, setVisible] = useState(false);
  return (
    <div className="flex flex-1 min-w-0 items-center gap-sm">
      <Input
        {...input}
        type={visible ? 'text' : 'password'}
        className={cn('flex-1 font-mono', className)}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        aria-label={label}
        disabled={disabled}
        autoComplete="off"
        spellCheck={false}
      />
      <IconButton
        type="button"
        label={visible ? `Hide the typed ${label}` : `Show the typed ${label}`}
        size="sm"
        onClick={() => setVisible((shown) => !shown)}
        disabled={disabled}
      >
        <Icon icon={visible ? EyeOff : Eye} size={14} />
      </IconButton>
    </div>
  );
};
