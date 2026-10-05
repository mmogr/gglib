import { createContext, FC, useContext } from 'react';
import { AttachmentPrimitive, ComposerPrimitive, useAttachment, useComposer } from '@assistant-ui/react';
import { ImagePlus, X } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { IconButton } from '../../ui/IconButton';
import type { ImageAttachment } from '../../../hooks/useGglibRuntime/imageAttachments';
import type { ImageInput } from '../../../hooks/useImageInput';
import { useImageUrl } from '../hooks/useImageUrl';
import { useContextReading } from '../hooks/useContextReading';
import { MessageActionsContext } from './MessageActionsContext';
import { imageCost } from './imageCost';

/**
 * Whether the chat's model takes images, why not, and its context, for every
 * composer of the thread: the page's and an edit's. Without a provider
 * nothing is offered.
 */
export const ImageInputContext = createContext<ImageInput>({ offered: false, reason: null, contextLength: null });

/**
 * One image in a composer: its thumbnail, how its upload stands or what it
 * costs, and Remove. Its share is of the context the ring reads, the size
 * the server that answered was launched with; before the conversation has
 * such a reading, of the context `ImageInputContext` gives.
 */
const ImageTile: FC = () => {
  const image = useAttachment() as ImageAttachment;
  const source = useContext(MessageActionsContext)?.source ?? 'this';
  const { contextLength } = useContext(ImageInputContext);
  const reading = useContextReading();
  const { url } = useImageUrl(image, source);
  const tokens = image.stored?.image_tokens;
  const caption =
    image.status.type === 'running' ? 'Uploading…'
      : image.status.type === 'incomplete' ? 'Not uploaded'
        : tokens !== undefined ? imageCost(tokens, reading?.size ?? contextLength) : '';
  return (
    <AttachmentPrimitive.Root className="relative flex flex-col gap-xs w-[112px]">
      <div className="h-[72px] w-full overflow-hidden rounded-base border border-border bg-surface">
        {url && <img src={url} alt={image.name} className="h-full w-full object-cover" />}
      </div>
      <span className="text-xs text-text-muted truncate" title={caption}>{caption}</span>
      <AttachmentPrimitive.Remove asChild>
        <IconButton label={`Remove ${image.name}`} size="sm" variant="secondary" className="absolute top-xs right-xs">
          <Icon icon={X} size={12} />
        </IconButton>
      </AttachmentPrimitive.Remove>
    </AttachmentPrimitive.Root>
  );
};

const TILES = { Image: ImageTile, Attachment: ImageTile };

/**
 * The images in the composer it is rendered in (the page's, or an edit's),
 * as a strip of tiles; nothing when it has none.
 */
export const ComposerImages: FC = () => {
  const count = useComposer((composer) => composer.attachments.length);
  if (count === 0) return null;
  return (
    <div className="flex flex-wrap gap-sm" role="group" aria-label="Attached images">
      <ComposerPrimitive.Attachments components={TILES} />
    </div>
  );
};

/**
 * Pick an image to attach, where the model takes them (`ImageInputContext`);
 * otherwise disabled, its tooltip saying why.
 */
export const AttachImageButton: FC<{ disabled?: boolean }> = ({ disabled }) => {
  const input = useContext(ImageInputContext);
  return input.offered ? (
    <ComposerPrimitive.AddAttachment asChild>
      <IconButton label="Attach an image" size="sm" disabled={disabled}>
        <Icon icon={ImagePlus} size={16} />
      </IconButton>
    </ComposerPrimitive.AddAttachment>
  ) : (
    <IconButton label="Attach an image" title={input.reason ?? undefined} size="sm" disabled>
      <Icon icon={ImagePlus} size={16} />
    </IconButton>
  );
};
