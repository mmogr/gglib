import { FC, ReactNode, useId } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { Icon } from "./Icon";

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
  size?: "sm" | "md" | "lg";
  preventClose?: boolean;
}

const sizeClassMap: Record<NonNullable<ModalProps["size"]>, string> = {
  sm: "modal-sm",
  md: "modal-md",
  lg: "modal-lg",
};

export const Modal: FC<ModalProps> = ({
  open,
  onClose,
  title,
  description,
  children,
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
        <DialogPrimitive.Overlay className="modal-overlay" />
        <DialogPrimitive.Content
          className={`modal ${sizeClassMap[size]}`}
          aria-describedby={hasDescription ? descriptionId : undefined}
          onPointerDownOutside={(event) => {
            if (preventClose) event.preventDefault();
          }}
          onEscapeKeyDown={(event) => {
            if (preventClose) event.preventDefault();
          }}
        >
          <div className="modal-header">
            <DialogPrimitive.Title className="modal-title">{title}</DialogPrimitive.Title>
            <DialogPrimitive.Close asChild>
              <button
                className="modal-close"
                onClick={onClose}
                aria-label="Close dialog"
                disabled={preventClose}
              >
                <Icon icon={X} size={14} />
              </button>
            </DialogPrimitive.Close>
          </div>
          {hasDescription ? (
            <DialogPrimitive.Description id={descriptionId} className="modal-description">
              {description}
            </DialogPrimitive.Description>
          ) : (
            <DialogPrimitive.Description style={visuallyHiddenStyle}>
              Dialog content
            </DialogPrimitive.Description>
          )}
          <div className="modal-body">{children}</div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
};
