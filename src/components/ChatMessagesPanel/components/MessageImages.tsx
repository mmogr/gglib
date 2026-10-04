import { FC, useContext, useState } from 'react';
import { MessagePrimitive, useAttachment, useMessage } from '@assistant-ui/react';
import { Button } from '../../ui/Button';
import { Modal } from '../../ui/Modal';
import type { ImageAttachment } from '../../../hooks/useGglibRuntime/imageAttachments';
import { useImageUrl } from '../hooks/useImageUrl';
import { MessageActionsContext } from './MessageActionsContext';

/**
 * One image of a user's turn: read from the chat's store (this machine's,
 * or the far machine's for a far chat) with the page's credential, shown
 * small, and enlarged in a dialog when clicked.
 */
const MessageImage: FC = () => {
  const image = useAttachment() as ImageAttachment;
  const source = useContext(MessageActionsContext)?.source ?? 'this';
  const { url, failed } = useImageUrl(image, source);
  const [open, setOpen] = useState(false);
  if (failed) return <span className="text-xs text-text-muted">The image could not be read.</span>;
  if (!url) return <span role="status" className="block h-[120px] w-[160px] rounded-base bg-surface" aria-label="Loading image" />;
  const size = image.stored ? `${image.stored.width} × ${image.stored.height}` : image.name;
  return (
    <>
      <Button variant="ghost" className="h-auto p-0 rounded-base overflow-hidden" onClick={() => setOpen(true)} aria-label={`Enlarge image, ${size}`}>
        <img src={url} alt={`Image, ${size}`} className="block max-h-[200px] max-w-[320px] object-contain" />
      </Button>
      <Modal open={open} onClose={() => setOpen(false)} title="Image" description={size} size="lg">
        <img src={url} alt={`Image, ${size}`} className="block max-h-[70vh] max-w-full mx-auto object-contain" />
      </Modal>
    </>
  );
};

const IMAGES = { Image: MessageImage, Attachment: MessageImage };

/** The images a user's turn carries, above its text; nothing when it has none. */
export const MessageImages: FC = () => {
  const count = useMessage((message) => message.attachments?.length ?? 0);
  if (count === 0) return null;
  return (
    <div className="flex flex-wrap gap-sm mb-sm" role="group" aria-label="Images">
      <MessagePrimitive.Attachments components={IMAGES} />
    </div>
  );
};
