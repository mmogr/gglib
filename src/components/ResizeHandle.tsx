import { FC, MouseEventHandler } from 'react';

interface ResizeHandleProps {
  onMouseDown: MouseEventHandler<HTMLDivElement>;
}

/** Vertical drag handle between two panels. Hidden on mobile, visible at md:. */
const ResizeHandle: FC<ResizeHandleProps> = ({ onMouseDown }) => (
  <div
    className="hidden md:block absolute top-0 right-[-2px] w-1 h-full cursor-col-resize bg-transparent z-base transition duration-200 hover:bg-primary active:bg-primary"
    onMouseDown={onMouseDown}
  />
);

export default ResizeHandle;
