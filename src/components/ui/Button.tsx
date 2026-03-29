import { forwardRef, type ReactNode } from "react";
import { cn } from "../../utils/cn";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "outline" | "danger" | "success" | "warning" | "link";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  leftIcon?: ReactNode;
  rightIcon?: ReactNode;
  isLoading?: boolean;
  iconOnly?: boolean;
  fullWidth?: boolean;
}

const baseStyles =
  "inline-flex items-center justify-center gap-2 rounded-base border border-transparent text-sm font-medium transition-all duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2 focus-visible:ring-offset-surface disabled:pointer-events-none disabled:opacity-60 disabled:cursor-not-allowed";

const variantStyles: Record<ButtonVariant, string> = {
  // Level 1 — Primary CTA. One per surface maximum.
  primary: "bg-primary text-white hover:bg-primary-hover",
  // Level 2 — Default action. Lifts off the page surface with a visible border.
  secondary: "bg-surface-elevated border-border text-text hover:bg-surface-hover hover:border-border-hover",
  // Level 3 — Emphasis without fill. Stronger rest border than secondary, fills on hover.
  outline: "bg-transparent border-border-hover text-text hover:bg-surface-elevated hover:border-primary",
  // Level 4 — Truly minimal. No border, no fill; only hover reveals the surface.
  ghost: "bg-transparent text-text-secondary border-transparent hover:text-text hover:bg-surface-elevated",
  // Semantic tints — soft warning state; reserves solid fills for destructive confirms.
  danger: "bg-danger-subtle text-danger border-danger-border hover:bg-danger/20",
  success: "bg-success-subtle text-success border-success-border hover:bg-success/20",
  warning: "bg-warning-subtle text-warning border-warning-border hover:bg-warning/20",
  // Inline text link — no background, no border.
  link: "bg-transparent text-primary h-auto p-0 hover:underline hover:text-primary-hover",
};

const sizeStyles: Record<ButtonSize, string> = {
  sm: "h-9 px-3 text-sm",
  md: "h-11 px-4 text-sm",
  lg: "h-12 px-5 text-base",
};

const iconOnlySizeStyles: Record<ButtonSize, string> = {
  sm: "h-9 w-9 p-0",
  md: "h-11 w-11 p-0",
  lg: "h-12 w-12 p-0",
};

const Spinner = () => (
  <svg
    className="animate-spin h-4 w-4"
    xmlns="http://www.w3.org/2000/svg"
    fill="none"
    viewBox="0 0 24 24"
  >
    <circle
      className="opacity-25"
      cx="12"
      cy="12"
      r="10"
      stroke="currentColor"
      strokeWidth="4"
    />
    <path
      className="opacity-75"
      fill="currentColor"
      d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
    />
  </svg>
);

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      variant = "primary",
      size = "md",
      className = "",
      leftIcon,
      rightIcon,
      isLoading = false,
      iconOnly = false,
      fullWidth = false,
      children,
      disabled,
      ...props
    },
    ref
  ) => {
    return (
      <button
        ref={ref}
        className={cn(
          baseStyles,
          variantStyles[variant],
          iconOnly ? iconOnlySizeStyles[size] : sizeStyles[size],
          fullWidth && "w-full",
          className,
        )}
        disabled={disabled || isLoading}
        {...props}
      >
        {isLoading ? (
          <Spinner />
        ) : (
          <>
            {leftIcon && !iconOnly ? (
              <span className="shrink-0 inline-flex items-center">{leftIcon}</span>
            ) : null}
            {iconOnly ? (
              children
            ) : (
              <span className="inline-flex items-center">{children}</span>
            )}
            {rightIcon && !iconOnly ? (
              <span className="shrink-0 inline-flex items-center">{rightIcon}</span>
            ) : null}
          </>
        )}
      </button>
    );
  }
);

Button.displayName = "Button";
