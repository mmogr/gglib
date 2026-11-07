/**
 * The picture sheet's frame: over the chat, holding the focus while open and
 * handing it back to the picture it was opened from on close. Esc, a click
 * outside it and × close it, once `mayClose` says they may.
 *
 * @module components/ChatMessagesPanel/components/SheetFrame
 */

import { FC, ReactNode, RefObject, useId } from 'react';
import * as DialogPrimitive from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { IconButton } from '../../ui/IconButton';

/** What closing asks first: `true` closes, `false` keeps the sheet open. */
export type MayClose = () => boolean | Promise<boolean>;

interface FrameProps {
  title: string;
  /** The picture's size in words, which describes the sheet. */
  size: string;
  from: RefObject<HTMLElement | null>;
  onClose: () => void;
  mayClose: MayClose;
  /** The picture, and anything drawn over or under it. */
  children: ReactNode;
  /** What is set beside the picture, where there is room; below it on a narrow window. */
  side?: ReactNode;
  footer?: ReactNode;
}

/** The sheet itself: a title, the picture's size, ×, the picture, what is beside it, and a footer. */
export const SheetFrame: FC<FrameProps> = ({ title, size, from, onClose, mayClose, children, side, footer }) => {
  const sizeId = useId();
  const ask = (event?: Event) => {
    event?.preventDefault();
    const answer = mayClose();
    if (answer === true) onClose();
    else if (answer !== false) void answer.then((yes) => yes && onClose());
  };
  return (
    <DialogPrimitive.Root open>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay data-modal-overlay className="fixed inset-0 z-modal-backdrop bg-black/60" />
        <DialogPrimitive.Content
          aria-describedby={sizeId}
          className="fixed left-1/2 top-1/2 z-modal flex max-h-[90vh] w-[calc(100%-32px)] max-w-[960px] -translate-x-1/2 -translate-y-1/2 animate-modal-slide-in flex-col rounded-lg bg-background-elevated shadow-2xl"
          onEscapeKeyDown={ask}
          onPointerDownOutside={ask}
          onCloseAutoFocus={(event) => {
            // Back to the picture it was opened from, while the thread still shows it.
            if (!from.current?.isConnected) return;
            event.preventDefault();
            from.current.focus();
          }}
        >
          <div className="flex items-center gap-sm px-md pt-md">
            <DialogPrimitive.Title className="m-0 text-base font-semibold text-text">{title}</DialogPrimitive.Title>
            <DialogPrimitive.Description id={sizeId} className="m-0 flex-1 text-xs tabular-nums text-text-secondary">
              {size}
            </DialogPrimitive.Description>
            <IconButton label="Close" onClick={() => ask()}>
              <Icon icon={X} size={14} />
            </IconButton>
          </div>
          <div className="flex min-h-0 flex-1 flex-col gap-md overflow-y-auto p-md md:flex-row md:items-start">
            <div className="flex min-w-0 flex-1 flex-col gap-sm">{children}</div>
            {side && <div className="flex flex-col gap-md text-sm text-text-secondary md:w-[260px] md:shrink-0">{side}</div>}
          </div>
          {footer && <div className="flex flex-wrap items-center gap-sm border-t border-border px-md py-sm">{footer}</div>}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
};

/** The box a picture is shown in at fit size: its own shape, never past 60% of the window's height. */
export const PictureBox: FC<{ width: number; height: number; children: ReactNode }> = ({ width, height, children }) => (
  <div className="relative mx-auto max-w-full select-none" style={{ aspectRatio: `${width} / ${height}`, maxHeight: '60vh' }}>
    {children}
  </div>
);
