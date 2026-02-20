import { FC, ReactNode, useId } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { Icon } from "./Icon";
import { cn } from "../../utils/cn";

// Styles that visually hide content while keeping it accessible to screen readers.
const visuallyHiddenStyle: React.CSSProperties = {
  position: "absolute",
  width: 1,
  height: 1,
  padding: 0,
  margin: -1,
  overflow: "hidden",
  clip: "rect(0, 0, 0, 0)",
  whiteSpace: "nowrap",
  borderWidth: 0,
};

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: ReactNode;
  children: ReactNode;
  /** Content pinned below the scroll area, outside overflow-y-auto. Rendered inside a flex row with border-t. */
  footer?: ReactNode;
  /** Extra classes merged onto the body wrapper (e.g. "p-0" to remove default padding). Uses tailwind-merge so "p-0" correctly overrides "p-lg". */
  bodyClassName?: string;
  /** Extra classes merged onto the footer wrapper (e.g. "justify-center" to override default "justify-end"). */
  footerClassName?: string;
  size?: "sm" | "md" | "lg";
  preventClose?: boolean;
}

const sizeClassMap: Record<NonNullable<ModalProps["size"]>, string> = {
  sm: "max-w-[400px]",
  md: "max-w-[600px]",
  lg: "max-w-[800px]",
};

export const Modal: FC<ModalProps> = ({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  bodyClassName,
  footerClassName,
  size = "md",
  preventClose = false,
}) => {
  const descriptionId = useId();
  const hasDescription = description !== undefined && description !== null;

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen && preventClose) return;
    if (!nextOpen) onClose();
  };

  return (
    <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 bg-black/70 flex items-center justify-center z-modal-backdrop p-base overflow-y-auto" />
        <DialogPrimitive.Content
          className={cn(
            "fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-background-elevated rounded-lg shadow-2xl w-full max-h-[90vh] flex flex-col z-modal animate-modal-slide-in",
            sizeClassMap[size],
          )}
          aria-describedby={hasDescription ? descriptionId : undefined}
          onPointerDownOutside={(event) => {
            if (preventClose) event.preventDefault();
          }}
          onEscapeKeyDown={(event) => {
            if (preventClose) event.preventDefault();
          }}
        >
          <div className="flex items-center justify-between p-lg border-b border-border shrink-0">
            <DialogPrimitive.Title className="text-xl font-semibold text-text m-0">{title}</DialogPrimitive.Title>
            <DialogPrimitive.Close asChild>
              <button
                className="w-[32px] h-[32px] rounded-base flex items-center justify-center bg-transparent text-text-secondary transition-all duration-200 cursor-pointer border-none shrink-0 hover:bg-background-hover hover:text-text"
                onClick={onClose}
                aria-label="Close dialog"
                disabled={preventClose}
              >
                <Icon icon={X} size={14} />
              </button>
            </DialogPrimitive.Close>
          </div>
          {hasDescription ? (
            <DialogPrimitive.Description id={descriptionId} className="px-lg pb-md text-sm text-text-secondary leading-normal">
              {description}
            </DialogPrimitive.Description>
          ) : (
            <DialogPrimitive.Description style={visuallyHiddenStyle}>
              Dialog content
            </DialogPrimitive.Description>
          )}
          <div className={cn("p-lg overflow-y-auto flex-1 min-h-0", bodyClassName)}>{children}</div>
          {footer != null && (
            <div className={cn("flex items-center justify-end gap-md p-lg border-t border-border shrink-0", footerClassName)}>
              {footer}
            </div>
          )}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
};
