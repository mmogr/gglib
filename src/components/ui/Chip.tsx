import { ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "../../utils/cn";
import { Icon } from "./Icon";

export type ChipVariant = "neutral" | "primary" | "success" | "warning" | "danger";
export type ChipSize = "sm" | "md";

export interface ChipProps {
  children: ReactNode;
  variant?: ChipVariant;
  size?: ChipSize;
  leftIcon?: ReactNode;
  /** Toggle-chip selected state (only meaningful with onClick). */
  selected?: boolean;
  /** When present the chip renders as a real button with hover/focus affordances. */
  onClick?: () => void;
  /** Renders a small remove control inside the chip (e.g. tag chips). */
  onRemove?: () => void;
  /** Accessible name for the remove control. Defaults to "Remove". */
  removeLabel?: string;
  className?: string;
  title?: string;
}

const variantClasses: Record<ChipVariant, string> = {
  neutral: "bg-surface-elevated text-text-secondary",
  primary: "bg-primary-subtle text-primary-light",
  success: "bg-success-subtle text-success",
  warning: "bg-warning-subtle text-warning",
  danger: "bg-danger-subtle text-danger",
};

const sizeClasses: Record<ChipSize, string> = {
  sm: "h-5 px-1.5 text-2xs rounded-sm gap-1",
  md: "h-6 px-2 text-xs rounded-base gap-1.5",
};

/**
 * Chip — small inline token for metadata, states, and filters.
 *
 * Non-interactive chips render as borderless <span>s so they never read
 * as buttons; passing onClick upgrades the chip to a real <button> with
 * hover and focus states.
 */
export function Chip({
  children,
  variant = "neutral",
  size = "md",
  leftIcon,
  selected = false,
  onClick,
  onRemove,
  removeLabel = "Remove",
  className,
  title,
}: ChipProps) {
  const shared = cn(
    "inline-flex items-center font-medium whitespace-nowrap max-w-full",
    sizeClasses[size],
    selected ? "bg-primary-subtle text-primary-light" : variantClasses[variant],
    className,
  );

  const content = (
    <>
      {leftIcon && <span className="inline-flex items-center shrink-0">{leftIcon}</span>}
      <span className="overflow-hidden text-ellipsis">{children}</span>
      {onRemove && (
        <button
          type="button"
          aria-label={removeLabel}
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="inline-flex items-center justify-center shrink-0 -mr-0.5 rounded-full cursor-pointer text-current opacity-60 hover:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary"
        >
          <Icon icon={X} size={size === "sm" ? 10 : 12} />
        </button>
      )}
    </>
  );

  if (onClick) {
    return (
      <button
        type="button"
        onClick={onClick}
        title={title}
        aria-pressed={selected}
        className={cn(
          shared,
          "cursor-pointer transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary",
          !selected && "hover:bg-surface-hover hover:text-text",
        )}
      >
        {content}
      </button>
    );
  }

  return (
    <span className={shared} title={title}>
      {content}
    </span>
  );
}
