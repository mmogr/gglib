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
  "inline-flex items-center justify-center gap-2 rounded-base border border-transparent text-sm font-medium transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2 focus-visible:ring-offset-surface disabled:pointer-events-none disabled:opacity-60 disabled:cursor-not-allowed";

const variantStyles: Record<ButtonVariant, string> = {
  primary: "bg-primary text-white hover:bg-primary-hover",
  secondary: "bg-background-secondary text-text border-border hover:border-primary",
  ghost: "bg-transparent text-text hover:bg-background-tertiary",
  outline: "bg-transparent text-text border-border hover:border-primary",
  danger: "bg-danger text-white hover:bg-danger-hover",
  success: "bg-success text-white hover:bg-success-hover",
  warning: "bg-warning text-white hover:bg-warning-hover",
  link: "bg-transparent text-primary h-auto p-0 hover:underline hover:text-primary-hover",
};

const sizeStyles: Record<ButtonSize, string> = {
  sm: "h-8 px-3 text-sm",
  md: "h-10 px-4 text-sm",
  lg: "h-11 px-5 text-base",
};

const iconOnlySizeStyles: Record<ButtonSize, string> = {
  sm: "h-8 w-8 p-0",
  md: "h-10 w-10 p-0",
  lg: "h-11 w-11 p-0",
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
