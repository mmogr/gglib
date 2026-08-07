import { ReactNode } from "react";
import { AlertCircle, AlertTriangle, CheckCircle2, Info } from "lucide-react";
import { cn } from "../../utils/cn";
import { Icon } from "./Icon";
import { IconButton } from "./IconButton";
import { X } from "lucide-react";

export type BannerVariant = "info" | "success" | "warning" | "danger";

export interface BannerProps {
  variant: BannerVariant;
  title?: ReactNode;
  children?: ReactNode;
  /** Action rendered on the right edge (e.g. a retry Button). */
  action?: ReactNode;
  onDismiss?: () => void;
  className?: string;
}

const variantConfig: Record<
  BannerVariant,
  { icon: typeof Info; container: string; iconColor: string }
> = {
  info: { icon: Info, container: "bg-primary-subtle", iconColor: "text-primary-light" },
  success: { icon: CheckCircle2, container: "bg-success-subtle", iconColor: "text-success" },
  warning: { icon: AlertTriangle, container: "bg-warning-subtle", iconColor: "text-warning" },
  danger: { icon: AlertCircle, container: "bg-danger-subtle", iconColor: "text-danger" },
};

/**
 * Banner — inline status/callout surface.
 *
 * Replaces the hand-rolled `bg-{x}-subtle border-{x}-border text-{x}`
 * triplet: tinted background and colored icon carry the semantics, text
 * stays readable neutrals, and there is no border per the design contracts.
 */
export function Banner({ variant, title, children, action, onDismiss, className }: BannerProps) {
  const config = variantConfig[variant];
  return (
    <div role={variant === "danger" ? "alert" : "status"} className={cn("flex items-start gap-sm rounded-md p-md", config.container, className)}>
      <Icon icon={config.icon} size={16} className={cn("shrink-0 mt-0.5", config.iconColor)} />
      <div className="flex-1 min-w-0 text-sm">
        {title && <div className="font-medium text-text">{title}</div>}
        {children && <div className={cn("text-text-secondary", title && "mt-0.5")}>{children}</div>}
      </div>
      {action && <div className="shrink-0">{action}</div>}
      {onDismiss && (
        <IconButton label="Dismiss" size="sm" onClick={onDismiss} className="-mt-1 -mr-1">
          <Icon icon={X} size={14} />
        </IconButton>
      )}
    </div>
  );
}
