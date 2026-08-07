import { forwardRef } from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "../../utils/cn";
import { Icon } from "./Icon";

const baseStyles = "w-full rounded-md border bg-background-input text-text text-sm transition-colors outline-none focus-visible:border-border-focus focus-visible:ring-2 focus-visible:ring-primary/10 hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-background cursor-pointer appearance-none";

const sizeStyles: Record<SelectSize, string> = {
  sm: "h-7 px-2 pr-7 text-xs",
  base: "h-8 px-3 pr-9 text-sm",
  lg: "h-9 px-3.5 pr-10 text-base",
};

const variantStyles: Record<SelectVariant, string> = {
  default: "border-border",
  error: "border-danger focus-visible:border-danger focus-visible:ring-danger/10",
};

const chevronOffset: Record<SelectSize, string> = {
  sm: "right-2",
  base: "right-3",
  lg: "right-3.5",
};

export type SelectSize = "sm" | "base" | "lg";
export type SelectVariant = "default" | "error";

export interface SelectProps extends Omit<React.SelectHTMLAttributes<HTMLSelectElement>, 'size'> {
  variant?: SelectVariant;
  size?: SelectSize;
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  (
    {
      variant = "default",
      size = "base",
      className = "",
      children,
      ...props
    },
    ref
  ) => {
    return (
      <span className="relative block w-full">
        <select
          ref={ref}
          className={cn(
            baseStyles,
            sizeStyles[size],
            variantStyles[variant],
            className,
          )}
          {...props}
        >
          {children}
        </select>
        {/* Token-compliant chevron: inherits text color instead of baking a hex into a data URI */}
        <Icon
          icon={ChevronDown}
          size={14}
          className={cn(
            "pointer-events-none absolute top-1/2 -translate-y-1/2 text-text-muted",
            chevronOffset[size],
          )}
        />
      </span>
    );
  }
);

Select.displayName = "Select";
