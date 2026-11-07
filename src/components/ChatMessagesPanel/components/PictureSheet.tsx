/**
 * The one picture sheet: a picture in the thread, opened over the chat at
 * fit size, from a reply's tool images or a question's own. A picture the
 * drawing tool made, in a chat that can keep another, is also fixed or drawn
 * again here (`PictureWork`); any other picture is only viewed.
 *
 * The sheet holds the focus while it is open and hands it back to the
 * picture it was opened from (`SheetFrame`). Esc, a click outside it and ×
 * close it, after asking when a picture drawn here would be lost unkept.
 *
 * @module components/ChatMessagesPanel/components/PictureSheet
 */

import type { FC, RefObject } from 'react';
import type { ToolImage } from '../../../types/messages';
import type { PictureActions } from './MessageActionsContext';
import { PictureWork } from './PictureWork';
import { SheetFrame } from './SheetFrame';

export interface PictureSheetProps {
  /** The picture as the thread shows it, already read. */
  url: string;
  /** Its size in words, `1024 × 1024`: its name to a reader. */
  size: string;
  /** The drawing tool's picture and its prompt, when a tool drew it. */
  drawn?: ToolImage;
  /** What the open chat fixes and redraws pictures with; absent where it can keep none. */
  pictures?: PictureActions;
  /** The thumbnail it was opened from, which takes the focus back. */
  from: RefObject<HTMLElement | null>;
  onClose: () => void;
}

export const PictureSheet: FC<PictureSheetProps> = ({ url, size, drawn, pictures, from, onClose }) => {
  if (drawn?.prompt && pictures) {
    return <PictureWork url={url} size={size} picture={drawn.image} prompt={drawn.prompt} actions={pictures} from={from} onClose={onClose} />;
  }
  return (
    <SheetFrame title="Picture" size={size} from={from} onClose={onClose} mayClose={() => true}>
      <img src={url} alt={`Image, ${size}`} className="mx-auto block max-h-[60vh] max-w-full object-contain" draggable={false} />
    </SheetFrame>
  );
};
