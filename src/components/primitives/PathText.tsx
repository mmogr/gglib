import { Fragment, type FC } from 'react';
import { cn } from '../../utils/cn';

/** A path's pieces, each ending with its separator: `C:\`, `Users\`, `me`. */
function pieces(path: string): string[] {
  return path.match(/[^\\/]*[\\/]+|[^\\/]+$/g) ?? [path];
}

/**
 * PathText - a file path in the monospace face that wraps after a `\` or a
 * `/`, so a folder's name stays whole. A name longer than the line still
 * breaks inside itself rather than overflow. The text copies as the path.
 */
export const PathText: FC<{ path: string; className?: string }> = ({ path, className }) => (
  <code className={cn('min-w-0 font-mono [overflow-wrap:anywhere]', className)}>
    {pieces(path).map((piece, i) => (
      <Fragment key={i}>
        {i > 0 && <wbr />}
        {piece}
      </Fragment>
    ))}
  </code>
);
