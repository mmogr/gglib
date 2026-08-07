import { forwardRef, type ReactNode } from "react";
import { Check } from "lucide-react";
import { cn } from "../../utils/cn";

export interface CheckboxProps
  extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "type" | "size"> {
  label?: ReactNode;
  description?: ReactNode;
  /** Extra classes for the outer label wrapper. */
  wrapperClassName?: string;
}

/**
 * Checkbox — design-system control replacing native accent-color checkboxes.
 *
 * The real input stays in the tree (sr-only) for full form/a11y semantics;
 * the visible box is styled via peer states.
 */
export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(
  ({ label, description, wrapperClassName, className, disabled, ...props }, ref) => {
    return (
      <label
        className={cn(
          "inline-flex items-start gap-sm",
          disabled ? "cursor-not-allowed opacity-60" : "cursor-pointer",
          wrapperClassName,
        )}
      >
        <span className="relative inline-flex shrink-0 mt-px">
          <input
            ref={ref}
            type="checkbox"
            disabled={disabled}
            className={cn("peer sr-only", className)}
            {...props}
          />
          <span
            aria-hidden="true"
            className={cn(
              "flex h-4 w-4 items-center justify-center rounded-sm border border-border bg-background-input transition-colors",
              "peer-hover:border-border-hover",
              "peer-checked:border-primary peer-checked:bg-primary",
              "peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-surface",
              // The icon is a descendant, not a sibling of the peer input —
              // stack the descendant selector on this (sibling) span.
              "[&>svg]:opacity-0 [&>svg]:transition-opacity peer-checked:[&>svg]:opacity-100",
            )}
          >
            <Check size={12} strokeWidth={3} className="text-text-inverse" />
          </span>
        </span>
        {(label || description) && (
          <span className="flex min-w-0 flex-col gap-0.5">
            {label && <span className="text-sm text-text leading-tight">{label}</span>}
            {description && <span className="text-xs text-text-muted">{description}</span>}
          </span>
        )}
      </label>
    );
  }
);

Checkbox.displayName = "Checkbox";
