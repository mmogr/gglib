import { FC, useContext } from 'react';
import { useThread } from '@assistant-ui/react';
import { Button } from '../../ui/Button';
import { BranchingContext } from './BranchingContext';

/** A reason as the line says it: the daemon's sentence, without its own full stop. */
function reasonOf(failure: string): string {
  return failure.trim().replace(/\.+$/, '');
}

/**
 * The 12 px line under a chat whose last reply did not come: "The reply
 * failed: <reason>." where the run read here failed (the reason is the
 * run's own, as the daemon ended it; `""` for a run that gave none), or "Nothing answers this yet." under
 * a question nothing answers, as a reply stopped before it began leaves
 * it, or a branch made from a question. Retry answers the question; a run
 * that failed after saving part of a reply is retried from that reply's
 * own line. Neither shows while a reply is being read.
 */
export const Unanswered: FC<{ failure?: string | null }> = ({ failure }) => {
  const branching = useContext(BranchingContext);
  const running = useThread({ optional: true })?.isRunning ?? false;
  const failed = failure !== undefined && failure !== null;
  if (running || (!failed && !branching?.answerable)) return null;
  const said = !failed ? 'Nothing answers this yet.' : failure ? `The reply failed: ${reasonOf(failure)}.` : 'The reply failed.';
  return (
    <div className="flex flex-wrap items-center gap-md text-xs text-text-muted">
      <p className="m-0">{said}</p>
      {branching?.answerable && (
        <Button
          variant="ghost"
          size="sm"
          className="h-auto p-0 text-xs font-normal hover:bg-transparent"
          onClick={() => void branching.retry()}
        >
          Retry
        </Button>
      )}
    </div>
  );
};
