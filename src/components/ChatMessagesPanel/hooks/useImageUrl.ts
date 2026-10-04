import { useEffect, useState } from 'react';
import { getTransport, type ChatSource } from '../../../services/transport';

/** An image to show: the file this page holds, else its id in a store. */
export interface ShownImage {
  id: string;
  file?: File;
}

/**
 * A `blob:` URL for `image`: of the file the page holds, or of its bytes
 * read from `source`'s store with this page's credential, which an `<img>`
 * pointed at the store could not send. Revoked when the image or the
 * component goes. `null` while it is read; `failed` when it could not be.
 */
export function useImageUrl(image: ShownImage, source: ChatSource): { url: string | null; failed: boolean } {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const { id, file } = image;

  useEffect(() => {
    let gone = false;
    let made: string | null = null;
    const bytes = file ? Promise.resolve<Blob>(file) : getTransport().fetchAttachmentBlob(source, id);
    bytes
      .then((blob) => {
        if (gone) return;
        made = URL.createObjectURL(blob);
        setUrl(made);
      })
      .catch(() => {
        if (!gone) setFailed(true);
      });
    return () => {
      gone = true;
      if (made) URL.revokeObjectURL(made);
      setUrl(null);
      setFailed(false);
    };
  }, [id, file, source]);

  return { url, failed };
}
