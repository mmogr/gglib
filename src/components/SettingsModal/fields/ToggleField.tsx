import { FC, ReactNode } from 'react';

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
  <div>
    <label htmlFor={id} className="flex items-center gap-sm cursor-pointer select-none">
      <input
        id={id}
        type="checkbox"
        className="w-[18px] h-[18px] accent-primary cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        disabled={disabled}
      />
      <span className="font-semibold text-text">{label}</span>
    </label>
    {children && <p className="text-text-secondary text-sm mt-xs">{children}</p>}
  </div>
);
