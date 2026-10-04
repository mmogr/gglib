/**
 * An image read as the store reads it (its header's size), and sent as it
 * is unless it is over the 2560 px edge or the 8 MiB cap.
 */

import { describe, it, expect, vi } from 'vitest';
import {
  MAX_IMAGE_BYTES,
  fitted,
  pixelSize,
  pixelSizeOf,
  prepareImage,
  type Downscale,
} from '../../../../src/hooks/useGglibRuntime/imagePrep';
import { png, pngFile } from '../../fixtures/fakeImageStore';

/** A JPEG: SOI, an APP0 segment, then a baseline frame header of `width` by `height`. */
function jpeg(width: number, height: number): Uint8Array {
  return new Uint8Array([
    0xff, 0xd8,
    0xff, 0xe0, 0x00, 0x04, 0x4a, 0x46,
    0xff, 0xc0, 0x00, 0x11, 0x08, height >> 8, height & 0xff, width >> 8, width & 0xff, 0x03,
  ]);
}

describe('pixelSize', () => {
  it('reads a PNG from its IHDR', () => {
    expect(pixelSize(png(1920, 1080))).toEqual({ width: 1920, height: 1080 });
  });

  it('reads a JPEG from its first frame header, past the segments before it', () => {
    expect(pixelSize(jpeg(4032, 3024))).toEqual({ width: 4032, height: 3024 });
  });

  it('reads nothing else, and no image of no size', () => {
    expect(pixelSize(new TextEncoder().encode('GIF89a......'))).toBeNull();
    expect(pixelSize(png(0, 10))).toBeNull();
    expect(pixelSize(jpeg(10, 10).subarray(0, 12))).toBeNull();
  });

  it('reads a file through the page', async () => {
    expect(await pixelSizeOf(pngFile(800, 600))).toEqual({ width: 800, height: 600 });
  });
});

describe('prepareImage', () => {
  it('keeps an image within the edge and the cap as it is', async () => {
    const downscale = vi.fn<Downscale>();
    const file = pngFile(2560, 2560);

    expect(await prepareImage(file, { width: 2560, height: 2560 }, downscale)).toBe(file);
    expect(downscale).not.toHaveBeenCalled();
  });

  it('downscales one over the edge, or over the cap', async () => {
    const smaller = pngFile(10, 10);
    const downscale = vi.fn<Downscale>(async () => smaller);
    const heavy = new File([new Uint8Array(MAX_IMAGE_BYTES + 1)], 'heavy.png', { type: 'image/png' });

    expect(await prepareImage(pngFile(2561, 10), { width: 2561, height: 10 }, downscale)).toBe(smaller);
    expect(await prepareImage(heavy, { width: 100, height: 100 }, downscale)).toBe(smaller);
    expect(downscale).toHaveBeenCalledTimes(2);
  });

  it('refuses one still over the cap once downscaled', async () => {
    const heavy = new File([new Uint8Array(MAX_IMAGE_BYTES + 1)], 'heavy.jpg', { type: 'image/jpeg' });

    await expect(prepareImage(heavy, { width: 100, height: 100 }, async () => heavy)).rejects.toMatchObject({
      code: 'image_too_large',
    });
  });
});

describe('fitted', () => {
  it('brings the longest side to 2560 px, keeping the shape', () => {
    expect(fitted({ width: 5120, height: 2880 })).toEqual({ width: 2560, height: 1440 });
    expect(fitted({ width: 1000, height: 4000 })).toEqual({ width: 640, height: 2560 });
    expect(fitted({ width: 800, height: 600 })).toEqual({ width: 800, height: 600 });
  });
});
