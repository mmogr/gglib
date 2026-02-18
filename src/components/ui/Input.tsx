import { forwardRef, type ReactNode } from "react";
import { cn } from "../../utils/cn";

const baseStyles = "w-full rounded-md border bg-background-input text-text text-sm transition-colors placeholder:text-text-disabled outline-none focus-visible:border-border-focus focus-visible:ring-2 focus-visible:ring-primary/10 hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-background";

const sizeStyles: Record<InputSize, string> = {
  sm: "h-8 px-2 text-xs",
  base: "h-10 px-3 text-sm",
  lg: "h-11 px-4 text-base",
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

export const Input = forwardRef<HTMLInputElement, InputProps>(
  (
    {
      variant = "default",
      size = "base",
      className = "",
      leftIcon,
      rightIcon,
      style,
      ...props
    },
    ref
  ) => {
    const hasLeftIcon = !!leftIcon;
    const hasRightIcon = !!rightIcon;
    
    const paddingStyle = {
      ...(hasLeftIcon && { paddingLeft: size === "sm" ? "2rem" : size === "lg" ? "2.75rem" : "2.5rem" }),
      ...(hasRightIcon && { paddingRight: size === "sm" ? "2rem" : size === "lg" ? "2.75rem" : "2.5rem" }),
      ...style,
    };

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
            className="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
            style={{
              left: size === "sm" ? "0.5rem" : size === "lg" ? "1rem" : "0.75rem",
            }}
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
            className,
          )}
          style={paddingStyle}
          {...props}
        />
        {rightIcon && (
          <div
            className="absolute right-3 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
            style={{
              right: size === "sm" ? "0.5rem" : size === "lg" ? "1rem" : "0.75rem",
            }}
          >
            {rightIcon}
          </div>
        )}
      </div>
    );
  }
);

Input.displayName = "Input";
