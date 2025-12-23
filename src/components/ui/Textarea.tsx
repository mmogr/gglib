import { forwardRef } from "react";

const baseStyles = "w-full rounded-md border bg-[var(--color-background-input)] text-[var(--color-text)] text-sm transition-colors placeholder:text-[var(--color-text-disabled)] outline-none focus-visible:border-[var(--color-border-focus)] focus-visible:ring-2 focus-visible:ring-[var(--color-primary)]/10 hover:border-[var(--color-border-hover)] disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-[var(--color-background)] resize-vertical leading-normal";

const sizeStyles: Record<TextareaSize, string> = {
  sm: "min-h-[80px] p-2 text-xs",
  base: "min-h-[100px] p-3 text-sm",
  lg: "min-h-[120px] p-4 text-base",
};

const variantStyles: Record<TextareaVariant, string> = {
  default: "border-[var(--color-border)]",
  error: "border-[var(--color-danger)] focus-visible:border-[var(--color-danger)] focus-visible:ring-[var(--color-danger)]/10",
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
        className={[
          baseStyles,
          sizeStyles[size],
          variantStyles[variant],
          className,
        ]
          .filter(Boolean)
          .join(" ")}
        {...props}
      />
    );
  }
);

Textarea.displayName = "Textarea";
