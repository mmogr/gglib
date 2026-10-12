/**
 * The mask a person brushes over a picture, and its bytes as the machine
 * reads them.
 *
 * A mask is the picture's own size in pixels, black where the picture is
 * kept and white where it is drawn again. The page keeps what was brushed
 * as strokes, in the picture's pixels, so the same strokes paint the
 * overlay a person sees and the mask the machine is sent, and undo is a
 * stroke taken off the end.
 *
 * The mask is painted on black, never on a clear canvas: the machine reads
 * a mask's colour and not its alpha, so a clear pixel would read as
 * whatever colour lies under it.
 *
 * @module pictureMask
 */

/** A point of the picture, in its own pixels from its top left. */
export interface MaskPoint {
  x: number;
  y: number;
}

/** One pass of the brush: where it went, how wide it was, and whether it painted or rubbed out. */
export interface MaskStroke {
  points: MaskPoint[];
  /** The brush's width, in the picture's pixels. */
  width: number;
  /** A stroke that takes paint off, where one before it put it on. */
  erase?: boolean;
}

/** What a canvas context is asked for here; `CanvasRenderingContext2D` is one. */
export type MaskContext = Pick<
  CanvasRenderingContext2D,
  | 'fillStyle'
  | 'strokeStyle'
  | 'lineWidth'
  | 'lineCap'
  | 'lineJoin'
  | 'globalCompositeOperation'
  | 'fillRect'
  | 'clearRect'
  | 'beginPath'
  | 'moveTo'
  | 'lineTo'
  | 'stroke'
  | 'arc'
  | 'fill'
>;

const KEPT = '#000000';
const REDRAWN = '#ffffff';

/** One stroke in `colour`: a round-ended line through its points, a dot for a single point. */
function paintStroke(ctx: MaskContext, stroke: MaskStroke, colour: string): void {
  const [first, ...rest] = stroke.points;
  if (!first) return;
  ctx.beginPath();
  if (rest.length === 0) {
    ctx.fillStyle = colour;
    ctx.arc(first.x, first.y, stroke.width / 2, 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  ctx.strokeStyle = colour;
  ctx.lineWidth = stroke.width;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  ctx.moveTo(first.x, first.y);
  for (const point of rest) ctx.lineTo(point.x, point.y);
  ctx.stroke();
}

/**
 * Paint the mask of `strokes` on `ctx`, a canvas `width` by `height`: black
 * all over, then each stroke in order, white for one that paints and black
 * for one that rubs out.
 */
export function paintMask(ctx: MaskContext, strokes: readonly MaskStroke[], width: number, height: number): void {
  ctx.fillStyle = KEPT;
  ctx.fillRect(0, 0, width, height);
  for (const stroke of strokes) paintStroke(ctx, stroke, stroke.erase ? KEPT : REDRAWN);
}

/**
 * Paint `strokes` on `ctx` as a person sees them over the picture: a clear
 * canvas, each stroke in `tint`, one that rubs out taking the tint off
 * again. The canvas is shown part clear, so the tint itself is solid and
 * strokes that cross do not darken.
 */
export function paintOverlay(
  ctx: MaskContext,
  strokes: readonly MaskStroke[],
  width: number,
  height: number,
  tint: string,
): void {
  ctx.globalCompositeOperation = 'source-over';
  ctx.clearRect(0, 0, width, height);
  for (const stroke of strokes) {
    ctx.globalCompositeOperation = stroke.erase ? 'destination-out' : 'source-over';
    paintStroke(ctx, stroke, tint);
  }
  ctx.globalCompositeOperation = 'source-over';
}

/**
 * Whether any of `strokes` paints. Paint a later stroke rubbed out entirely
 * still counts here; the machine refuses a mask with no white in it.
 */
export function marksSomething(strokes: readonly MaskStroke[]): boolean {
  return strokes.some((stroke) => !stroke.erase && stroke.points.length > 0);
}

/**
 * Where on the picture a pointer is, in the picture's pixels: `client` is
 * the pointer's place in the window, `box` the picture's on screen, and
 * `natural` the picture's own size. Held inside the picture.
 */
export function picturePoint(
  client: MaskPoint,
  box: { left: number; top: number; width: number; height: number },
  natural: { width: number; height: number },
): MaskPoint {
  const along = (at: number, start: number, shown: number, real: number) =>
    shown > 0 ? Math.min(real, Math.max(0, ((at - start) / shown) * real)) : 0;
  return {
    x: along(client.x, box.left, box.width, natural.width),
    y: along(client.y, box.top, box.height, natural.height),
  };
}

/** `blob`'s bytes as base64, with no `data:` prefix: read as the page reads a file. */
export function base64Of(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result);
      resolve(url.slice(url.indexOf(',') + 1));
    };
    reader.onerror = () => reject(reader.error ?? new Error('The picture could not be read.'));
    reader.readAsDataURL(blob);
  });
}

/** The PNG `base64` holds, as a blob. */
export function pngOf(base64: string): Blob {
  const text = atob(base64);
  const bytes = new Uint8Array(text.length);
  for (let at = 0; at < text.length; at += 1) bytes[at] = text.charCodeAt(at);
  return new Blob([bytes], { type: 'image/png' });
}

/**
 * The mask of `strokes` for a picture `width` by `height`, as a PNG: painted
 * on a canvas of the picture's own size, off the page.
 */
export async function maskPng(strokes: readonly MaskStroke[], width: number, height: number): Promise<Blob> {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('This browser cannot paint a mask.');
  paintMask(ctx, strokes, width, height);
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('The mask could not be made.'))), 'image/png');
  });
}
