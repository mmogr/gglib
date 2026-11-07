import { FC } from 'react';
import { Banner } from '../../ui/Banner';

interface ChatStatusBannersProps {
  chatError: string | null;
}

/**
 * The banner that sits above the message list: the current chat error.
 *
 * A model unloaded or crashed mid-chat has no banner here, and no toast:
 * the composer is replaced by the line that says so (`ModelGoneLine`),
 * which is the read-only lock until H2
 * (`feat(gui)/chat-without-lock`) removes it.
 */
export const ChatStatusBanners: FC<ChatStatusBannersProps> = ({ chatError }) =>
  chatError ? (
    <Banner variant="danger" className="shrink-0">
      {chatError}
    </Banner>
  ) : null;
