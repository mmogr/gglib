import { forwardRef, type ReactNode } from "react";

const baseStyles = "inline-flex items-center justify-center gap-2 rounded-md border text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-primary)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--color-surface,transparent)] disabled:pointer-events-none disabled:opacity-60";

const variantStyles: Record<ButtonVariant, string> = {
  primary:
    "bg-[var(--color-primary)] text-white border-transparent hover:bg-[color-mix(in_oklab,var(--color-primary),white_12%)]",
  secondary:
    "bg-[var(--color-background-secondary)] text-[var(--color-text)] border-[var(--color-border)] hover:border-[var(--color-primary)]",
  ghost:
    "bg-transparent text-[var(--color-text)] border-transparent hover:bg-[var(--color-background-tertiary)]",
  outline:
    "bg-transparent text-[var(--color-text)] border-[var(--color-border)] hover:border-[var(--color-primary)]",
  danger:
    "bg-[#ef4444] text-white border-transparent hover:bg-[color-mix(in_oklab,#ef4444,white_12%)]",
};

const sizeStyles: Record<ButtonSize, string> = {
  sm: "h-8 px-3 text-sm",
  md: "h-10 px-4 text-sm",
  lg: "h-11 px-5 text-base",
};

export type ButtonVariant = "primary" | "secondary" | "ghost" | "outline" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  leftIcon?: ReactNode;
  rightIcon?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ variant = "primary", size = "md", className = "", leftIcon, rightIcon, children, ...props }, ref) => {
    return (
      <button
        ref={ref}
        className={[baseStyles, variantStyles[variant], sizeStyles[size], className].filter(Boolean).join(" ")}
        {...props}
      >
        {leftIcon ? <span className="shrink-0">{leftIcon}</span> : null}
        <span className="inline-flex items-center">{children}</span>
        {rightIcon ? <span className="shrink-0">{rightIcon}</span> : null}
      </button>
    );
  }
);

Button.displayName = "Button";
