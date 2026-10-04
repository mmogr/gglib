/**
 * An image made ready to upload: a PNG or a JPEG whose size its header
 * says, no wider or taller than 2560 px and no larger than the 8 MiB the
 * store takes. One that is already so goes as it is; one that is not is
 * redrawn smaller by a downscaler the caller passes, PNG kept as PNG when it
 * fits and JPEG otherwise.
 *
 * Read in the browser as the store reads it (`image_size.rs`), so what the
 * page sends is what the store accepts.
 *
 * @module imagePrep
 */

/** The longest side a sent image has, in pixels. */
export const MAX_IMAGE_EDGE = 2560;

/** The most bytes one stored image may be (`MAX_IMAGE_BYTES`). */
export const MAX_IMAGE_BYTES = 8 * 1024 * 1024;

/** What the store takes, as the file picker is told. */
export const IMAGE_ACCEPT = 'image/png,image/jpeg';

export interface PixelSize {
  width: number;
  height: number;
}

/** Redraw `image`, of `size`, smaller: within `MAX_IMAGE_EDGE` and the cap. */
export type Downscale = (image: File, size: PixelSize) => Promise<File>;

/** Why an image cannot be sent: the store's code for the same refusal. */
export class ImageRefusal extends Error {
  constructor(readonly code: 'unsupported_image' | 'image_too_large', message: string) {
    super(message);
    this.name = 'ImageRefusal';
  }
}

function sized(width: number, height: number): PixelSize | null {
  return width > 0 && height > 0 ? { width, height } : null;
}

function png(bytes: Uint8Array): PixelSize | null {
  const ihdr = String.fromCharCode(...bytes.subarray(12, 16));
  if (bytes.length < 24 || ihdr !== 'IHDR') return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return sized(view.getUint32(16), view.getUint32(20));
}

/** A JPEG's size: its segments walked to the first frame header. */
function jpeg(bytes: Uint8Array): PixelSize | null {
  const u16 = (at: number) => (bytes[at] << 8) | bytes[at + 1];
  let at = 2;
  while (at + 1 < bytes.length) {
    if (bytes[at] !== 0xff) return null;
    const marker = bytes[at + 1];
    if (marker === 0xff) {
      at += 1;
    } else if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) {
      at += 2;
    } else if (marker === 0x00 || marker === 0xd9 || marker === 0xda) {
      return null;
    } else if (marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker)) {
      return at + 9 <= bytes.length ? sized(u16(at + 7), u16(at + 5)) : null;
    } else {
      const length = u16(at + 2);
      if (length < 2) return null;
      at += 2 + length;
    }
  }
  return null;
}

/** The size of the PNG or JPEG `bytes` is, or `null` for anything else. */
export function pixelSize(bytes: Uint8Array): PixelSize | null {
  if (bytes[0] === 0x89 && String.fromCharCode(...bytes.subarray(1, 4)) === 'PNG') return png(bytes);
  if (bytes[0] === 0xff && bytes[1] === 0xd8) return jpeg(bytes);
  return null;
}

/** The bytes of `file`, read with a `FileReader`, which every page has. */
function bytesOf(file: Blob): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(new Uint8Array(reader.result as ArrayBuffer));
    reader.onerror = () => reject(reader.error ?? new Error('The file could not be read.'));
    reader.readAsArrayBuffer(file);
  });
}

/** The size of the PNG or JPEG `file` is, read from its header, or `null`. */
export async function pixelSizeOf(file: Blob): Promise<PixelSize | null> {
  try {
    return pixelSize(await bytesOf(file));
  } catch {
    return null;
  }
}

/**
 * `file`, of `size`, as it is sent: as it is when it is within the edge and
 * the cap, else what `downscale` makes of it.
 *
 * @throws ImageRefusal — `image_too_large` for an image still over the cap
 *   once redrawn.
 */
export async function prepareImage(file: File, size: PixelSize, downscale: Downscale): Promise<File> {
  if (Math.max(size.width, size.height) <= MAX_IMAGE_EDGE && file.size <= MAX_IMAGE_BYTES) return file;
  const smaller = await downscale(file, size);
  if (smaller.size > MAX_IMAGE_BYTES) {
    throw new ImageRefusal('image_too_large', `${file.name || 'The image'} is over 8 MiB even made smaller.`);
  }
  return smaller;
}

/** The size `size` is drawn at: its longest side at most `MAX_IMAGE_EDGE`. */
export function fitted(size: PixelSize): PixelSize {
  const scale = Math.min(1, MAX_IMAGE_EDGE / Math.max(size.width, size.height));
  return {
    width: Math.max(1, Math.round(size.width * scale)),
    height: Math.max(1, Math.round(size.height * scale)),
  };
}

/** The browser's downscaler: drawn on a canvas, PNG when it fits, else JPEG. */
export const downscaleInBrowser: Downscale = async (image, size) => {
  const { width, height } = fitted(size);
  const bitmap = await createImageBitmap(image);
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  canvas.getContext('2d')?.drawImage(bitmap, 0, 0, width, height);
  bitmap.close();
  const encode = (type: string, quality?: number) =>
    new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, type, quality));
  let blob = image.type === 'image/png' ? await encode('image/png') : null;
  if (!blob || blob.size > MAX_IMAGE_BYTES) blob = await encode('image/jpeg', 0.9);
  if (!blob) throw new ImageRefusal('unsupported_image', `${image.name || 'The image'} could not be redrawn.`);
  const name = blob.type === 'image/jpeg' ? `${image.name.replace(/\.[^.]*$/, '') || 'image'}.jpg` : image.name;
  return new File([blob], name, { type: blob.type });
};
