import { createContext, FC, useCallback, useContext } from 'react';
import { AttachmentPrimitive, ComposerPrimitive, useAttachment, useComposer, useComposerRuntime } from '@assistant-ui/react';
import { X } from 'lucide-react';
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

/** A tile's state while its image is not yet in the store. */
const UPLOADING = 'uploading…';
const NOT_UPLOADED = 'not uploaded';

/**
 * One image in a composer: its thumbnail, "uploading…" or "not uploaded"
 * while it is not in the store, what it costs on hover, and × to remove it.
 * Its share is of the context the ring reads, the size the server that
 * answered was launched with; before the conversation has such a reading,
 * of the context `ImageInputContext` gives.
 */
const ImageTile: FC = () => {
  const image = useAttachment() as ImageAttachment;
  const source = useContext(MessageActionsContext)?.source ?? 'this';
  const { contextLength } = useContext(ImageInputContext);
  const reading = useContextReading();
  const { url } = useImageUrl(image, source);
  const tokens = image.stored?.image_tokens;
  const state = image.status.type === 'running' ? UPLOADING : image.status.type === 'incomplete' ? NOT_UPLOADED : null;
  const cost = state === null && tokens !== undefined ? imageCost(tokens, reading?.size ?? contextLength) : null;
  return (
    <AttachmentPrimitive.Root className="group relative flex w-[72px] flex-col gap-xs" title={cost ?? undefined}>
      <div className="h-[72px] w-full overflow-hidden rounded-base bg-surface">
        {url && <img src={url} alt={image.name} className="h-full w-full object-cover" />}
      </div>
      {state && <span className="truncate text-xs text-text-secondary">{state}</span>}
      {cost && (
        <span className="pointer-events-none absolute left-0 top-full z-popover mt-xs w-max rounded-base bg-surface-elevated px-sm py-xs text-xs text-text-secondary opacity-0 shadow-lg tabular-nums group-hover:opacity-100">
          {cost}
        </span>
      )}
      <AttachmentPrimitive.Remove asChild>
        <IconButton label={`Remove ${image.name}`} size="sm" variant="secondary" className="absolute right-xs top-xs">
          <Icon icon={X} size={12} />
        </IconButton>
      </AttachmentPrimitive.Remove>
    </AttachmentPrimitive.Root>
  );
};

const TILES = { Image: ImageTile, Attachment: ImageTile };

/**
 * The images in the composer it is rendered in (the page's, or an edit's),
 * as a strip of tiles above the text; nothing when it has none.
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
 * Pick images to attach to the composer it is called in, as
 * `ComposerPrimitive.AddAttachment` does: a hidden file picker on the page,
 * taking what the composer accepts, gone once it has answered.
 */
export function useAddImage(): () => void {
  const composer = useComposerRuntime();
  return useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = true;
    input.hidden = true;
    const accept = composer.getState().attachmentAccept;
    if (accept !== '*') input.accept = accept;
    document.body.appendChild(input);
    input.onchange = () => {
      for (const file of input.files ?? []) void composer.addAttachment(file).catch(() => {});
      input.remove();
    };
    input.oncancel = () => input.remove();
    input.click();
  }, [composer]);
}
