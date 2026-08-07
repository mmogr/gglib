import { forwardRef, type ReactNode } from "react";
import { cn } from "../../utils/cn";

const baseStyles = "w-full rounded-md border bg-background-input text-text text-sm transition-colors placeholder:text-text-disabled outline-none focus-visible:border-border-focus focus-visible:ring-2 focus-visible:ring-primary/10 hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-background";

const sizeStyles: Record<InputSize, string> = {
  sm: "h-7 px-2 text-xs",
  base: "h-8 px-3 text-sm",
  lg: "h-9 px-3.5 text-base",
};

const variantStyles: Record<InputVariant, string> = {
  default: "border-border",
  error: "border-danger focus-visible:border-danger focus-visible:ring-danger/10",
};

export type InputSize = "sm" | "base" | "lg";
export type InputVariant = "default" | "error";

export interface InputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size'> {
  variant?: InputVariant;
  size?: InputSize;
  leftIcon?: ReactNode;
  rightIcon?: ReactNode;
}

const leftIconPadding: Record<InputSize, string> = {
  sm: "pl-7",
  base: "pl-9",
  lg: "pl-10",
};

const rightIconPadding: Record<InputSize, string> = {
  sm: "pr-7",
  base: "pr-9",
  lg: "pr-10",
};

const leftIconOffset: Record<InputSize, string> = {
  sm: "left-2",
  base: "left-3",
  lg: "left-4",
};

const rightIconOffset: Record<InputSize, string> = {
  sm: "right-2",
  base: "right-3",
  lg: "right-4",
};

export const Input = forwardRef<HTMLInputElement, InputProps>(
  (
    {
      variant = "default",
      size = "base",
      className = "",
      leftIcon,
      rightIcon,
      ...props
    },
    ref
  ) => {
    if (!leftIcon && !rightIcon) {
      return (
        <input
          ref={ref}
          className={cn(
            baseStyles,
            sizeStyles[size],
            variantStyles[variant],
            className,
          )}
          {...props}
        />
      );
    }

    return (
      <div className="relative w-full">
        {leftIcon && (
          <div
            className={cn(
              "absolute top-1/2 -translate-y-1/2 text-text-muted pointer-events-none",
              leftIconOffset[size],
            )}
          >
            {leftIcon}
          </div>
        )}
        <input
          ref={ref}
          className={cn(
            baseStyles,
            sizeStyles[size],
            variantStyles[variant],
            leftIcon && leftIconPadding[size],
            rightIcon && rightIconPadding[size],
            className,
          )}
          {...props}
        />
        {rightIcon && (
          <div
            className={cn(
              "absolute top-1/2 -translate-y-1/2 text-text-muted pointer-events-none",
              rightIconOffset[size],
            )}
          >
            {rightIcon}
          </div>
        )}
      </div>
    );
  }
);

Input.displayName = "Input";
