import * as React from 'react';

import { cn } from './utils';

const ScrollArea = React.forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div ref={ref} data-slot="scroll-area" className={cn('relative overflow-auto', className)} {...props} />
  )
);
ScrollArea.displayName = 'ScrollArea';

export { ScrollArea };
