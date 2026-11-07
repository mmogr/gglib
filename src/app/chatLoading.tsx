/**
 * What a chat route shows while it learns which chat to open, and when it
 * cannot learn it: the list of chats could not be read, so the chat's model
 * is not known, and guessing one would send to the wrong model.
 *
 * @module app/chatLoading
 */

import { Button } from '../components/ui/Button';

export const CHAT_LOADING = (
  <div className="flex flex-col h-full w-full overflow-hidden">
    <div role="status" className="loading-chat">Loading chat…</div>
  </div>
);

/** The chat's list could not be read, twice: say so, and offer to read again. */
export function ChatUnread({ onRetry }: { onRetry: () => void }) {
  return (
    <div className="flex flex-col h-full w-full overflow-hidden">
      <div role="alert" className="loading-chat">
        The chat couldn't be read.{' '}
        <Button variant="link" size="sm" onClick={onRetry}>
          Try again
        </Button>
      </div>
    </div>
  );
}
