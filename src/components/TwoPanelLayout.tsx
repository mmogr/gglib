import { forwardRef, PointerEventHandler, ReactNode } from 'react';
import ResizeHandle from './ResizeHandle';
import { cn } from '../utils/cn';

interface TwoPanelLayoutProps {
  leftWidth: number;
  onResizeStart: PointerEventHandler<HTMLDivElement>;
  onKeyboardResize?: (delta: number) => void;
  left: ReactNode;
  right: ReactNode;
  className?: string;
  leftClassName?: string;
  rightClassName?: string;
  /**
   * When true, hides the layout at all breakpoints.
   * Uses both `hidden` and `md:hidden` so tailwind-merge can remove
   * the responsive `md:grid` base class, preventing cascade conflicts.
   */
  isHidden?: boolean;
}

/**
 * Responsive two-panel grid layout with a draggable resize handle.
 * Stacks vertically on mobile, switches to a side-by-side grid at md:.
 */
const TwoPanelLayout = forwardRef<HTMLDivElement, TwoPanelLayoutProps>(
  ({ leftWidth, onResizeStart, onKeyboardResize, left, right, className, leftClassName, rightClassName, isHidden }, ref) => (
    <div
      ref={ref}
      className={cn(
        'flex flex-col md:grid md:grid-cols-2 md:gap-0 md:h-full md:overflow-hidden',
        isHidden && 'hidden md:hidden',
        className,
      )}
      style={{ gridTemplateColumns: `${leftWidth}% ${100 - leftWidth}%` }}
    >
      <div className={cn('relative flex flex-col overflow-hidden md:h-full md:min-h-0', leftClassName)}>
        {left}
        <ResizeHandle onPointerDown={onResizeStart} onKeyboardResize={onKeyboardResize} />
      </div>
      <div className={cn('relative flex flex-col overflow-hidden md:h-full md:min-h-0', rightClassName)}>
        {right}
      </div>
    </div>
  ),
);

TwoPanelLayout.displayName = 'TwoPanelLayout';

export default TwoPanelLayout;
