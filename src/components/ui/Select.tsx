import { forwardRef } from "react";

const baseStyles = "w-full rounded-md border bg-[var(--color-background-input)] text-[var(--color-text)] text-sm transition-colors outline-none focus-visible:border-[var(--color-border-focus)] focus-visible:ring-2 focus-visible:ring-[var(--color-primary)]/10 hover:border-[var(--color-border-hover)] disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-[var(--color-background)] cursor-pointer appearance-none bg-no-repeat";

const sizeStyles: Record<SelectSize, string> = {
  sm: "h-8 px-2 pr-8 text-xs",
  base: "h-10 px-3 pr-10 text-sm",
  lg: "h-11 px-4 pr-12 text-base",
};

const variantStyles: Record<SelectVariant, string> = {
  default: "border-[var(--color-border)]",
  error: "border-[var(--color-danger)] focus-visible:border-[var(--color-danger)] focus-visible:ring-[var(--color-danger)]/10",
};

// SVG chevron-down icon as data URI
const chevronIcon = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23888888' d='M6 9L1 4h10z'/%3E%3C/svg%3E";

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
      style,
      children,
      ...props
    },
    ref
  ) => {
    const bgPosition = size === "sm" ? "right 0.5rem center" : size === "lg" ? "right 1rem center" : "right 0.75rem center";
    
    return (
      <select
        ref={ref}
        className={[
          baseStyles,
          sizeStyles[size],
          variantStyles[variant],
          className,
        ]
          .filter(Boolean)
          .join(" ")}
        style={{
          backgroundImage: `url("${chevronIcon}")`,
          backgroundPosition: bgPosition,
          ...style,
        }}
        {...props}
      >
        {children}
      </select>
    );
  }
);

Select.displayName = "Select";
