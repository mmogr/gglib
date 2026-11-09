import { FC } from 'react';
import { Banner } from '../../ui/Banner';

interface ChatStatusBannersProps {
  chatError: string | null;
}

/**
 * The banner that sits above the message list: the current chat error.
 *
 * A model unloaded or crashed mid-chat has no banner here, and no toast:
 * one line above the live composer says what a send does about it
 * (`SendNoteLine`).
 */
export const ChatStatusBanners: FC<ChatStatusBannersProps> = ({ chatError }) =>
  chatError ? (
    <Banner variant="danger" className="shrink-0">
      {chatError}
    </Banner>
  ) : null;
