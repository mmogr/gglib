import { forwardRef } from "react";
import type { ComponentPropsWithoutRef } from "react";
import type { LucideIcon } from "lucide-react";

interface IconProps extends ComponentPropsWithoutRef<"span"> {
  icon: LucideIcon;
  size?: number;
  strokeWidth?: number;
}

export const Icon = forwardRef<HTMLSpanElement, IconProps>(
  ({ icon: IconComponent, size = 16, strokeWidth = 1.6, className = "", ...props }, ref) => {
    return (
      <span
        ref={ref}
        className={`inline-flex items-center justify-center ${className}`}
        aria-hidden="true"
        {...props}
      >
        <IconComponent size={size} strokeWidth={strokeWidth} />
      </span>
    );
  }
);

Icon.displayName = "Icon";
