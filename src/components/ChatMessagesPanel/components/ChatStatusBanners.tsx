import { FC } from 'react';
import { Banner } from '../../ui/Banner';

interface ChatStatusBannersProps {
  chatError: string | null;
  isServerConnected: boolean;
}

/**
 * The two banners that sit above the message list: the current chat error and
 * the read-only warning shown while the server is down.
 */
export const ChatStatusBanners: FC<ChatStatusBannersProps> = ({ chatError, isServerConnected }) => (
  <>
    {chatError && (
      <Banner variant="danger" className="shrink-0">
        {chatError}
      </Banner>
    )}

    {!isServerConnected && (
      <Banner variant="warning" className="shrink-0">
        Server not running — Chat is read-only
      </Banner>
    )}
  </>
);
