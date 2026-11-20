/**
 * What a chat route shows while it learns which chat to open, and when it
 * cannot learn it: the list of chats could not be read, so the chat's model
 * is not known, and guessing one would send to the wrong model; or a read
 * the fallback waits on failed, said in its own words with Retry; or there
 * is no model to open a chat on, said with the way to the library.
 *
 * @module app/chatLoading
 */

import { Button } from '../components/ui/Button';

export const CHAT_LOADING = (
  <div className="flex flex-col h-full w-full overflow-hidden">
    <div role="status" className="loading-chat">Loading chat…</div>
  </div>
);

/**
 * What the chat waits on (the server list, the library, the settings) could
 * not be read: say why, in the error's own words, and offer to read again.
 */
export function ChatWaitFailed({ words, onRetry }: { words: string; onRetry: () => void }) {
  return (
    <div className="flex flex-col h-full w-full overflow-hidden">
      <div role="alert" className="loading-chat">
        {`Can't open the chat yet: ${words}`}{' '}
        <Button variant="link" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </div>
    </div>
  );
}

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

/**
 * A chat that names no model, with none used, set as the default or loaded,
 * and no only chat model in the library: say what to do, with the way to
 * the library. `several` is whether the library holds more than one chat
 * model, to pick from, or none, to get.
 */
export function ChatNoModel({ several, onLibrary }: { several: boolean; onLibrary: () => void }) {
  return (
    <div className="flex flex-col h-full w-full overflow-hidden">
      <div role="status" className="loading-chat">
        {several ? 'Pick a model to chat with: open one in the library and press Chat.' : 'No model to chat with yet. Get one in the library.'}{' '}
        <Button variant="link" size="sm" onClick={onLibrary}>
          Open the library
        </Button>
      </div>
    </div>
  );
}
