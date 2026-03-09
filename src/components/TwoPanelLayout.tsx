import { forwardRef, MouseEventHandler, ReactNode } from 'react';
import ResizeHandle from './ResizeHandle';
import { cn } from '../utils/cn';

interface TwoPanelLayoutProps {
  leftWidth: number;
  onResizeStart: MouseEventHandler<HTMLDivElement>;
  left: ReactNode;
  right: ReactNode;
  className?: string;
  leftClassName?: string;
  rightClassName?: string;
}

/**
 * Responsive two-panel grid layout with a draggable resize handle.
 * Stacks vertically on mobile, switches to a side-by-side grid at md:.
 */
const TwoPanelLayout = forwardRef<HTMLDivElement, TwoPanelLayoutProps>(
  ({ leftWidth, onResizeStart, left, right, className, leftClassName, rightClassName }, ref) => (
    <div
      ref={ref}
      className={cn(
        'flex flex-col md:grid md:grid-cols-2 md:gap-0 md:h-full md:overflow-hidden',
        className,
      )}
      style={{ gridTemplateColumns: `${leftWidth}% ${100 - leftWidth}%` }}
    >
      <div className={cn('relative flex flex-col overflow-hidden md:h-full md:min-h-0', leftClassName)}>
        {left}
        <ResizeHandle onMouseDown={onResizeStart} />
      </div>
      <div className={cn('relative flex flex-col overflow-hidden md:h-full md:min-h-0', rightClassName)}>
        {right}
      </div>
    </div>
  ),
);

TwoPanelLayout.displayName = 'TwoPanelLayout';

export default TwoPanelLayout;
