import { forwardRef } from "react";
import { cn } from "../../utils/cn";

const baseStyles = "w-full rounded-md border bg-background-input text-text text-sm transition-colors placeholder:text-text-disabled outline-none focus-visible:border-border-focus focus-visible:ring-2 focus-visible:ring-primary/10 hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-background resize-vertical leading-normal";

const sizeStyles: Record<TextareaSize, string> = {
  sm: "min-h-[80px] p-2 text-xs",
  base: "min-h-[100px] p-3 text-sm",
  lg: "min-h-[120px] p-4 text-base",
};

const variantStyles: Record<TextareaVariant, string> = {
  default: "border-border",
  error: "border-danger focus-visible:border-danger focus-visible:ring-danger/10",
};

export type TextareaSize = "sm" | "base" | "lg";
export type TextareaVariant = "default" | "error";

export interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  variant?: TextareaVariant;
  size?: TextareaSize;
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  (
    {
      variant = "default",
      size = "base",
      className = "",
      ...props
    },
    ref
  ) => {
    return (
      <textarea
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
);

Textarea.displayName = "Textarea";
