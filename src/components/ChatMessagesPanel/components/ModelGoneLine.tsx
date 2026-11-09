import { FC } from 'react';
import { Button } from '../../ui/Button';
import { StatusMark } from '../../status/StatusMark';
import { RESIDENCY_WORDS } from '../../status/words';

/** Why the chat's model is not there: unloaded (stopped), or it stopped on its own (crashed). */
export type ModelGone = 'unloaded' | 'crashed';

interface ModelGoneLineProps {
  gone: ModelGone;
  modelName: string;
  starting: string | null;
  onLoadAgain?: () => void;
  onShowLog?: () => void;
}

/**
 * The line that stands in for the composer while this machine's model is
 * gone. Unloaded: "<model> was unloaded. [Load it again]", which loads it
 * and keeps the chat;
 * crashed: "<model> stopped unexpectedly · Show log". No banner, no toast.
 *
 * This is the read-only lock: H2 (`feat(gui)/chat-without-lock`) removes
 * it, once chat goes through the endpoint and a send loads the model.
 */
export const ModelGoneLine: FC<ModelGoneLineProps> = ({ gone, modelName, starting, onLoadAgain, onShowLog }) => {
  const line = 'm-0 flex items-center gap-sm py-md text-sm text-text-secondary';
  if (starting !== null) {
    return (
      <p role="status" className={line}>
        <StatusMark kind="changing" />
        {RESIDENCY_WORDS.loading} {starting}…
      </p>
    );
  }
  if (gone === 'crashed') {
    return (
      <p role="status" className={line}>
        <StatusMark kind="failed" />
        {modelName} stopped unexpectedly
        {onShowLog && (
          <>
            <span aria-hidden>·</span>
            <Button variant="link" size="sm" className="text-sm text-text-secondary" onClick={onShowLog}>
              Show log
            </Button>
          </>
        )}
      </p>
    );
  }
  return (
    <p role="status" className={line}>
      {modelName} was unloaded.
      {onLoadAgain && (
        <Button variant="primary" size="sm" onClick={onLoadAgain}>
          Load it again
        </Button>
      )}
    </p>
  );
};
