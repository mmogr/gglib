import { forwardRef } from "react";
import { Button, type ButtonProps } from "./Button";

export interface IconButtonProps
  extends Omit<ButtonProps, "iconOnly" | "leftIcon" | "rightIcon" | "aria-label"> {
  /**
   * Accessible name — required. Icon-only controls are otherwise anonymous
   * to assistive tech; this sets both aria-label and the hover tooltip.
   */
  label: string;
}

/**
 * Icon-only button. Thin wrapper over Button that makes the accessible
 * name impossible to forget.
 */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
  ({ label, variant = "ghost", children, title, ...props }, ref) => (
    <Button
      ref={ref}
      iconOnly
      variant={variant}
      aria-label={label}
      title={title ?? label}
      {...props}
    >
      {children}
    </Button>
  )
);

IconButton.displayName = "IconButton";
