import { FC } from 'react';
import { Button } from '../../ui/Button';
import { StatusMark } from '../../status/StatusMark';

/** What a send does first, as Rust words it (`status/sendLine`), and whether the model stopped on its own. */
export interface SendNote {
  line: string;
  crashed: boolean;
}

/**
 * One 12 px line above a live composer: "Qwen3-4B isn't loaded · sending
 * loads it", "The endpoint is Off · sending turns it on". A model that
 * stopped on its own is marked failed, and offers its log ("· Show log").
 * Never a lock, a banner or a toast: the composer under it sends.
 */
export const SendNoteLine: FC<{ note: SendNote; onShowLog?: () => void }> = ({ note, onShowLog }) => (
  <p role="status" className="m-0 mb-xs flex flex-wrap items-center gap-x-xs px-sm text-xs text-text-muted">
    {note.crashed && <StatusMark kind="failed" className="text-xs" />}
    <span>{note.line}</span>
    {note.crashed && onShowLog && (
      <>
        <span aria-hidden>·</span>
        <Button variant="link" size="sm" className="h-auto p-0 text-xs text-text-muted" onClick={onShowLog}>
          Show log
        </Button>
      </>
    )}
  </p>
);
