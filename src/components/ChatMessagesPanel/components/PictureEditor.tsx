import { FC, PointerEvent, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '../../ui/Button';
import { Modal } from '../../ui/Modal';
import { Select } from '../../ui/Select';
import { Textarea } from '../../ui/Textarea';
import { getTransport } from '../../../services/transport';
import type { RedrawProgress } from '../../../services/transport/api/images';
import type { AttachmentInfo } from '../../../types/generated/AttachmentInfo';
import type { ImageModelRow } from '../../../types/generated/ImageModelRow';
import { imageFamilyLabel } from '../../../utils/imageFamily';
import { marksSomething, paintOverlay, picturePoint, type MaskStroke } from '../../../utils/pictureMask';
import { usePictureRedraw, type RedrawSource } from '../hooks/usePictureRedraw';

/** What a person is doing to the picture: brushing a part to fix, or drawing all of it again. */
type Mode = 'fix' | 'redo';

/** How far from the picture each starts: a fix may change its part a lot, a redo keeps most of the picture. */
const START_STRENGTH: Record<Mode, number> = { fix: 0.75, redo: 0.4 };

/** The colour the brushed part is shown in, at the canvas's own part-clear. */
const TINT = '#ff4d4d';

/** A blob's `blob:` URL, for as long as the blob is shown. */
function useBlobUrl(blob: Blob | null): string | null {
  const url = useMemo(() => (blob ? URL.createObjectURL(blob) : null), [blob]);
  useEffect(
    () => () => {
      if (url) URL.revokeObjectURL(url);
    },
    [url],
  );
  return url;
}

/** Where a render has got to, in words. */
export function progressLine(progress: RedrawProgress | null): string {
  switch (progress?.stage) {
    case 'queued':
      return progress.position ? `Waiting for the GPU, place ${progress.position} in line…` : 'Waiting for the GPU…';
    case 'sampling':
      return progress.step && progress.total ? `Drawing, step ${progress.step} of ${progress.total}…` : 'Drawing…';
    case 'decoding':
    case 'finishing':
      return 'Finishing the picture…';
    default:
      return 'Loading the image model…';
  }
}

export interface PictureEditorProps {
  open: boolean;
  onClose: () => void;
  /** The picture to fix or redo: one a tool drew, in this machine's store. */
  picture: AttachmentInfo;
  /** The picture, already read, as the thread shows it. */
  url: string;
  /** What the picture was drawn from: the prompt its tool call gave. */
  prompt: string;
  conversationId: number;
  /** This machine's image models; a choice is offered when more than one can draw. */
  models: readonly ImageModelRow[];
  /** The image model the chat has chosen, which the choice starts at; `null` for the machine's own. */
  picked: number | null;
  /** Told once the chat holds the newer picture. */
  onKept: () => void | Promise<void>;
}

/**
 * A picture a tool drew, fixed or drawn again. Fix: brush over the part
 * that is wrong, and only that part is drawn again; the rest comes back as
 * it was. Redo: the whole picture is drawn again from itself, as far from
 * it as the slider says. The result is shown beside the choice to keep it,
 * which adds it to the chat after the picture it came from, or to try
 * again. Nothing is kept until Keep is pressed, and a picture that has been
 * drawn is not lost to a stray click: the dialog closes on Keep or Discard.
 */
export const PictureEditor: FC<PictureEditorProps> = ({ open, onClose, picture, url, prompt, conversationId, models, picked, onKept }) => {
  const [mode, setMode] = useState<Mode>('fix');
  const [strokes, setStrokes] = useState<MaskStroke[]>([]);
  const [brush, setBrush] = useState(() => Math.round(Math.max(picture.width, picture.height) * 0.06));
  const [erasing, setErasing] = useState(false);
  const [words, setWords] = useState(prompt);
  const [strength, setStrength] = useState(START_STRENGTH.fix);
  // A choice the machine no longer lists, or that lacks a file, is not one
  // the menu can show: the machine's own choice stands in for it.
  const [model, setModel] = useState<number | null>(() =>
    models.some((row) => row.id === picked && row.complete) ? picked : null,
  );
  const [comparing, setComparing] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null);
  /** The pointer brushing now, by id; `null` for none. */
  const painting = useRef<number | null>(null);

  const source = useMemo<RedrawSource>(
    () => ({
      id: picture.id,
      width: picture.width,
      height: picture.height,
      blob: () => getTransport().fetchAttachmentBlob('this', picture.id),
    }),
    [picture.id, picture.width, picture.height],
  );
  // Kept: the dialog closes whether or not the chat could be shown again.
  const { state, start, stop, discard, keep } = usePictureRedraw(source, conversationId, async () => {
    try {
      await onKept();
    } finally {
      onClose();
    }
  });
  const drawn = state.phase === 'drawn' || state.phase === 'keeping' ? state.picture : null;
  const drawnUrl = useBlobUrl(drawn);
  const busy = state.phase === 'drawing' || state.phase === 'keeping';
  const editing = state.phase === 'idle' || state.phase === 'failed';
  const usable = models.filter((row) => row.complete);

  useEffect(() => {
    const ctx = canvas.current?.getContext('2d');
    if (ctx) paintOverlay(ctx, mode === 'fix' ? strokes : [], picture.width, picture.height, TINT);
  }, [strokes, mode, picture.width, picture.height, editing]);

  const pointOf = (event: PointerEvent<HTMLCanvasElement>) =>
    picturePoint({ x: event.clientX, y: event.clientY }, event.currentTarget.getBoundingClientRect(), picture);
  const begin = (event: PointerEvent<HTMLCanvasElement>) => {
    // One brush: the main button, the first finger.
    if (event.button > 0 || event.isPrimary === false) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    painting.current = event.pointerId;
    const stroke: MaskStroke = { points: [pointOf(event)], width: brush, ...(erasing && { erase: true }) };
    setStrokes((before) => [...before, stroke]);
  };
  const extend = (event: PointerEvent<HTMLCanvasElement>) => {
    if (painting.current !== event.pointerId) return;
    const point = pointOf(event);
    setStrokes((before) => {
      const last = before[before.length - 1];
      const at = last?.points[last.points.length - 1];
      // A move that has not moved adds nothing: a line of no length paints nothing.
      if (!last || (at && at.x === point.x && at.y === point.y)) return before;
      return [...before.slice(0, -1), { ...last, points: [...last.points, point] }];
    });
  };
  const end = (event: PointerEvent<HTMLCanvasElement>) => {
    if (painting.current === event.pointerId) painting.current = null;
  };

  const choose = (next: Mode) => {
    painting.current = null;
    setMode(next);
    setStrength(START_STRENGTH[next]);
  };
  const ask = () => start({ prompt: words.trim(), strength, strokes: mode === 'fix' ? strokes : [], model });
  const ready = words.trim().length > 0 && (mode === 'redo' || marksSomething(strokes));
  const blocked = !words.trim()
    ? 'Say what the picture should show.'
    : !ready
      ? 'Brush over the part to fix first.'
      : undefined;

  const footer = drawn ? (
    <>
      <Button
        variant="ghost"
        onPointerDown={() => setComparing(true)}
        onPointerUp={() => setComparing(false)}
        onPointerLeave={() => setComparing(false)}
        onKeyDown={(event) => event.key === ' ' && setComparing(true)}
        onKeyUp={() => setComparing(false)}
        onBlur={() => setComparing(false)}
        aria-pressed={comparing}
      >
        Hold to see the original
      </Button>
      <span className="flex-1" />
      <Button variant="ghost" onClick={discard} disabled={busy}>Discard</Button>
      <Button variant="secondary" onClick={ask} disabled={busy}>Try again</Button>
      <Button variant="primary" onClick={() => void keep()} disabled={busy}>
        {state.phase === 'keeping' ? 'Keeping…' : 'Keep'}
      </Button>
    </>
  ) : state.phase === 'drawing' ? (
    <>
      <span role="status" className="flex-1 text-sm text-text-secondary">{progressLine(state.progress)}</span>
      <Button variant="secondary" onClick={stop}>Stop</Button>
    </>
  ) : (
    <>
      {/* In words: a disabled button shows no title, and touch has none. */}
      <span className="flex-1 text-sm text-text-muted">{blocked}</span>
      <Button variant="ghost" onClick={onClose}>Cancel</Button>
      <Button variant="primary" onClick={ask} disabled={!ready}>
        {mode === 'fix' ? 'Fix this part' : 'Draw it again'}
      </Button>
    </>
  );

  const frame = state.phase === 'drawing' ? state.progress?.frame : undefined;
  const shown = drawn && !comparing ? drawnUrl : url;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={drawn ? 'Keep this picture?' : 'Fix or redo this picture'}
      description={`${picture.width} × ${picture.height}`}
      size="lg"
      preventClose={busy || drawn !== null}
      footer={footer}
      footerClassName="justify-start"
    >
      {/* Beside the picture where there is room, so nothing a person sets is
          under the fold; below it on a narrow window. */}
      <div className="flex flex-col gap-md md:flex-row md:items-start">
        <div className="flex min-w-0 flex-1 flex-col gap-md">
          <div className="relative mx-auto max-w-full select-none" style={{ aspectRatio: `${picture.width} / ${picture.height}`, maxHeight: '55vh' }}>
            {shown && <img src={shown} alt={drawn && !comparing ? 'The picture drawn again' : 'The picture'} className="block h-full w-full object-contain" draggable={false} />}
            {frame && (
              <img src={`data:image/png;base64,${frame}`} alt="The picture so far" className="absolute inset-0 h-full w-full object-contain opacity-90" />
            )}
            {editing && mode === 'fix' && (
              <canvas
                ref={canvas}
                width={picture.width}
                height={picture.height}
                aria-label="Brush over the part to fix"
                className="absolute inset-0 h-full w-full cursor-crosshair touch-none opacity-50"
                onPointerDown={begin}
                onPointerMove={extend}
                onPointerUp={end}
                onPointerCancel={end}
              />
            )}
          </div>
          {state.phase === 'failed' && <p role="alert" className="m-0 text-sm text-danger">{state.message}</p>}
          {state.phase === 'drawn' && state.problem && <p role="alert" className="m-0 text-sm text-danger">{state.problem}</p>}
        </div>
        {editing && (
          <div className="flex flex-col gap-md text-sm text-text-secondary md:w-[260px] md:shrink-0">
            <div className="flex flex-wrap gap-sm" role="group" aria-label="What to do">
              <Button variant={mode === 'fix' ? 'primary' : 'ghost'} size="sm" aria-pressed={mode === 'fix'} onClick={() => choose('fix')}>
                Fix a part
              </Button>
              <Button variant={mode === 'redo' ? 'primary' : 'ghost'} size="sm" aria-pressed={mode === 'redo'} onClick={() => choose('redo')}>
                Redo the whole picture
              </Button>
            </div>
            {mode === 'fix' && (
              <div className="flex flex-col gap-sm">
                <label className="flex items-center gap-sm">
                  Brush
                  <input
                    type="range"
                    className="min-w-0 flex-1 accent-primary"
                    min={Math.round(Math.max(picture.width, picture.height) * 0.01)}
                    max={Math.round(Math.max(picture.width, picture.height) * 0.25)}
                    value={brush}
                    onChange={(event) => setBrush(Number(event.target.value))}
                    aria-label="Brush size"
                  />
                </label>
                <div className="flex flex-wrap gap-sm">
                  <Button variant={erasing ? 'primary' : 'ghost'} size="sm" aria-pressed={erasing} onClick={() => setErasing(!erasing)}>
                    Rub out
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setStrokes((before) => before.slice(0, -1))} disabled={strokes.length === 0}>
                    Undo
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setStrokes([])} disabled={strokes.length === 0}>
                    Clear
                  </Button>
                </div>
              </div>
            )}
            <label className="flex flex-col gap-xs">
              {mode === 'fix' ? 'What the brushed part should show' : 'What the picture should show'}
              <Textarea value={words} onChange={(event) => setWords(event.target.value)} rows={4} />
            </label>
            <label className="flex flex-col gap-xs">
              How much to change
              <input
                type="range"
                className="accent-primary"
                min={0.1}
                max={1}
                step={0.05}
                value={strength}
                onChange={(event) => setStrength(Number(event.target.value))}
                aria-label="How much to change"
              />
              <span className="flex justify-between text-xs text-text-muted" aria-hidden>
                <span>a little</span>
                <span>a lot</span>
              </span>
            </label>
            {usable.length > 1 && (
              <label className="flex flex-col gap-xs">
                Drawn with
                <Select
                  size="sm"
                  aria-label="Image model"
                  value={model ?? 'default'}
                  onChange={(event) => setModel(event.target.value === 'default' ? null : Number(event.target.value))}
                >
                  <option value="default">The default image model</option>
                  {usable.map((row) => (
                    <option key={row.id} value={row.id}>
                      {[row.name, imageFamilyLabel(row.family) ?? row.family].join(' · ')}
                    </option>
                  ))}
                </Select>
              </label>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
};
