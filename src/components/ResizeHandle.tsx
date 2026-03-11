import { FC, PointerEventHandler, KeyboardEvent } from 'react';

interface ResizeHandleProps {
  onPointerDown: PointerEventHandler<HTMLDivElement>;
  onKeyboardResize?: (delta: number) => void;
}

const KEYBOARD_STEP = 2; // percentage points per arrow key press

/** Vertical drag handle between two panels. Hidden on mobile, visible at md:. */
const ResizeHandle: FC<ResizeHandleProps> = ({ onPointerDown, onKeyboardResize }) => {
  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!onKeyboardResize) return;
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      onKeyboardResize(-KEYBOARD_STEP);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      onKeyboardResize(KEYBOARD_STEP);
    }
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize panels"
      tabIndex={0}
      className="hidden md:block absolute top-0 right-[-2px] w-1 h-full cursor-col-resize bg-transparent z-base transition duration-200 hover:bg-primary active:bg-primary focus-visible:bg-primary"
      onPointerDown={onPointerDown}
      onKeyDown={handleKeyDown}
    />
  );
};

export default ResizeHandle;
