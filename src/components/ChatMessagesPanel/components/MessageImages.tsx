import { FC, useContext, useState } from 'react';
import { MessagePrimitive, useAttachment, useMessage } from '@assistant-ui/react';
import { Button } from '../../ui/Button';
import { Modal } from '../../ui/Modal';
import type { ImageAttachment } from '../../../hooks/useGglibRuntime/imageAttachments';
import type { ToolImage } from '../../../types/messages';
import { useImageUrl, type ShownImage } from '../hooks/useImageUrl';
import { MessageActionsContext } from './MessageActionsContext';
import { PictureEditor } from './PictureEditor';

/**
 * One image in the thread: read from the chat's store (this machine's, or
 * the far machine's for a far chat) with the page's credential, shown
 * small, and enlarged in a dialog when clicked. `size` names it to the
 * reader. A picture the drawing tool made (`drawn`) can be fixed or drawn
 * again from the dialog, where the chat allows it.
 */
const EnlargeableImage: FC<{ image: ShownImage; size: string; drawn?: ToolImage }> = ({ image, size, drawn }) => {
  const actions = useContext(MessageActionsContext);
  const source = actions?.source ?? 'this';
  const { url, failed } = useImageUrl(image, source);
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  if (failed) return <span className="text-xs text-text-muted">The image could not be read.</span>;
  if (!url) return <span role="status" className="block h-[120px] w-[160px] rounded-base bg-surface" aria-label="Loading image" />;
  const pictures = drawn?.prompt ? actions?.pictures : undefined;
  return (
    <>
      <Button variant="ghost" className="h-auto p-0 rounded-base overflow-hidden" onClick={() => setOpen(true)} aria-label={`Enlarge image, ${size}`}>
        <img src={url} alt={`Image, ${size}`} className="block max-h-[200px] max-w-[320px] object-contain" />
      </Button>
      <Modal
        open={open}
        onClose={() => setOpen(false)}
        title="Image"
        description={size}
        size="lg"
        footer={
          pictures && (
            <Button
              variant="secondary"
              onClick={() => {
                setOpen(false);
                setEditing(true);
              }}
            >
              Fix or redo…
            </Button>
          )
        }
      >
        <img src={url} alt={`Image, ${size}`} className="block max-h-[70vh] max-w-full mx-auto object-contain" />
      </Modal>
      {pictures && drawn?.prompt && editing && (
        <PictureEditor
          open
          onClose={() => setEditing(false)}
          picture={drawn.image}
          url={url}
          prompt={drawn.prompt}
          conversationId={pictures.conversationId}
          models={pictures.models}
          picked={pictures.picked}
          onKept={pictures.onKept}
        />
      )}
    </>
  );
};

/** One image of a user's turn, as its attachment. */
const MessageImage: FC = () => {
  const image = useAttachment() as ImageAttachment;
  const size = image.stored ? `${image.stored.width} × ${image.stored.height}` : image.name;
  return <EnlargeableImage image={image} size={size} />;
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

/**
 * The images a reply's tools made, in the order they made them, as a strip
 * in the reply's body, each read from the store by its id as a user's image
 * is; nothing when they made none. A picture kept after a fix or a redo is
 * on its tool's row after the one it came from, so it follows it here.
 */
export const ToolImages: FC<{ images: readonly ToolImage[] }> = ({ images }) => {
  if (images.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-sm mt-sm" role="group" aria-label="Images from tools">
      {images.map((drawn, index) => (
        <EnlargeableImage
          key={`${index}-${drawn.image.id}`}
          image={{ id: drawn.image.id }}
          size={`${drawn.image.width} × ${drawn.image.height}`}
          drawn={drawn}
        />
      ))}
    </div>
  );
};
