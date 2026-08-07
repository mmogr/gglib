import { FC, ReactNode } from 'react';
import { Checkbox } from '../../ui/Checkbox';

interface ToggleFieldProps {
  id: string;
  label: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
  /** Explanatory copy rendered under the label. */
  children?: ReactNode;
}

/**
 * Checkbox with a bold label and an explanatory paragraph underneath.
 *
 * The settings form had this markup inlined in every place a boolean setting
 * appeared, so each new toggle re-stated the same Tailwind classes and got a
 * chance to drift from the others. Every boolean setting in the modal renders
 * through here instead.
 */
export const ToggleField: FC<ToggleFieldProps> = ({
  id,
  label,
  checked,
  onChange,
  disabled = false,
  children,
}) => (
  <Checkbox
    id={id}
    checked={checked}
    onChange={(event) => onChange(event.target.checked)}
    disabled={disabled}
    label={<span className="font-medium">{label}</span>}
    description={children}
  />
);
