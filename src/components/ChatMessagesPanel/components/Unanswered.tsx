import { FC } from 'react';
import { useThread } from '@assistant-ui/react';
import { Button } from '../../ui/Button';
import { TurnRow } from './TurnRow';
import { TurnWho } from './TurnMargin';

/**
 * The row under a chat that ends in a question nothing answers, as a reply
 * that failed or was stopped before it began leaves it, or a branch made
 * from a question: Retry answers it. Not while a reply is being read.
 */
export const Unanswered: FC<{ answerable: boolean; retry: () => Promise<void> }> = ({ answerable, retry }) => {
  const running = useThread({ optional: true })?.isRunning ?? false;
  if (!answerable || running) return null;
  return (
    <TurnRow
      who={<TurnWho name="No reply" />}
      body={
        <div className="flex flex-wrap items-center gap-md">
          <p className="m-0 text-text-muted">Nothing answers this question yet.</p>
          <Button variant="secondary" size="sm" onClick={() => void retry()}>
            Retry
          </Button>
        </div>
      }
    />
  );
};
