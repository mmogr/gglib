import { FC } from 'react';
import { Button } from '../../ui/Button';
import { Banner } from '../../ui/Banner';

interface ChatStatusBannersProps {
  chatError: string | null;
  isServerConnected: boolean;
  onClose?: () => void;
}

/**
 * The two banners that sit above the message list: the current chat error and
 * the read-only warning shown while the server is down.
 */
export const ChatStatusBanners: FC<ChatStatusBannersProps> = ({
  chatError,
  isServerConnected,
  onClose,
}) => (
  <>
    {chatError && (
      <Banner variant="danger" className="shrink-0">
        {chatError}
      </Banner>
    )}

    {!isServerConnected && (
      <Banner
        variant="warning"
        className="shrink-0"
        action={
          onClose && (
            <Button variant="secondary" size="sm" onClick={onClose}>
              Close
            </Button>
          )
        }
      >
        Server not running — Chat is read-only
      </Banner>
    )}
  </>
);
