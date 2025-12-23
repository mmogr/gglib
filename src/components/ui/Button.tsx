import { forwardRef, type ReactNode } from "react";

const baseStyles = "inline-flex items-center justify-center gap-2 rounded-md border text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-primary)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--color-surface,transparent)] disabled:pointer-events-none disabled:opacity-60";

const variantStyles: Record<ButtonVariant, string> = {
  primary:
    "bg-[var(--color-primary)] text-white border-transparent hover:bg-[var(--color-primary-hover)]",
  secondary:
    "bg-[var(--color-background-secondary)] text-[var(--color-text)] border-[var(--color-border)] hover:border-[var(--color-primary)]",
  ghost:
    "bg-transparent text-[var(--color-text)] border-transparent hover:bg-[var(--color-background-tertiary)]",
  outline:
    "bg-transparent text-[var(--color-text)] border-[var(--color-border)] hover:border-[var(--color-primary)]",
  danger:
    "bg-[var(--color-danger)] text-white border-transparent hover:bg-[var(--color-danger-hover)]",
  success:
    "bg-[var(--color-success)] text-white border-transparent hover:bg-[var(--color-success-hover)]",
  warning:
    "bg-[var(--color-warning)] text-white border-transparent hover:bg-[var(--color-warning-hover)]",
  link:
    "bg-transparent text-[var(--color-primary)] border-transparent hover:underline hover:text-[var(--color-primary-hover)] h-auto p-0",
};

const sizeStyles: Record<ButtonSize, string> = {
  sm: "h-8 px-3 text-sm",
  md: "h-10 px-4 text-sm",
  lg: "h-11 px-5 text-base",
};

const iconOnlyStyles: Record<ButtonSize, string> = {
  sm: "h-8 w-8 p-0",
  md: "h-10 w-10 p-0",
  lg: "h-11 w-11 p-0",
};

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

const Spinner = () => (
  <svg
    className="animate-spin h-4 w-4"
    xmlns="http://www.w3.org/2000/svg"
    fill="none"
    viewBox="0 0 24 24"
    style={{
      animation: "spin 1s linear infinite",
    }}
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
    <style>{`
      @keyframes spin {
        from { transform: rotate(0deg); }
        to { transform: rotate(360deg); }
      }
    `}</style>
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
    const sizeClass = iconOnly ? iconOnlyStyles[size] : sizeStyles[size];
    const widthClass = fullWidth ? "w-full" : "";
    const gapClass = iconOnly ? "" : "gap-2";

    return (
      <button
        ref={ref}
        className={[
          baseStyles,
          variantStyles[variant],
          sizeClass,
          widthClass,
          gapClass,
          className,
        ]
          .filter(Boolean)
          .join(" ")}
        disabled={disabled || isLoading}
        {...props}
      >
        {isLoading ? (
          <Spinner />
        ) : (
          <>
            {leftIcon && !iconOnly ? <span className="shrink-0">{leftIcon}</span> : null}
            {iconOnly ? (
              children
            ) : (
              <span className="inline-flex items-center">{children}</span>
            )}
            {rightIcon && !iconOnly ? <span className="shrink-0">{rightIcon}</span> : null}
          </>
        )}
      </button>
    );
  }
);

Button.displayName = "Button";
