/**
 * A picture the drawing tool made, in the picture sheet: viewed, a part of
 * it fixed, or all of it drawn again.
 *
 * View: the picture, with Fix a part and Redo. Fix a part: the brush row
 * (`PictureBrush`) marks what is wrong, and only that part is drawn again;
 * the rest comes back as it was. Redo: the whole picture is drawn again from
 * itself. Each takes the prompt the picture was drawn from, how much to
 * change, and, where more than one image model can draw, which draws it.
 * While it draws: where the render is, from the daemon's progress, in words
 * and a bar, with Stop. After: Hold to see the original, then Keep, which
 * saves it as a new picture after the one it came from (the original
 * stays), Try again with the same values, or Discard. Nothing is kept until
 * Keep, and closing with a drawn picture unkept asks first. A failure is
 * said in the sheet, in the daemon's words.
 *
 * @module components/ChatMessagesPanel/components/PictureWork
 */

import { FC, RefObject, useEffect, useMemo, useRef, useState } from 'react';
import { confirm, dismissConfirm } from '../../../app/confirm';
import { StatusMark } from '../../status/StatusMark';
import { Button } from '../../ui/Button';
import { Select } from '../../ui/Select';
import { Textarea } from '../../ui/Textarea';
import { getTransport } from '../../../services/transport';
import type { RedrawProgress } from '../../../services/transport/api/images';
import type { AttachmentInfo } from '../../../types/generated/AttachmentInfo';
import { imageFamilyLabel } from '../../../utils/imageFamily';
import { marksSomething } from '../../../utils/pictureMask';
import { usePictureRedraw, type RedrawSource } from '../hooks/usePictureRedraw';
import type { PictureActions } from './MessageActionsContext';
import { BrushCanvas, BrushRow, useBrush } from './PictureBrush';
import { PictureBox, SheetFrame, type MayClose } from './SheetFrame';

/** What the sheet is doing with the picture: showing it, fixing a part, or drawing all of it again. */
type Step = 'view' | 'fix' | 'redo';

/** How far from the picture each starts: a fix may change its part a lot, a redo keeps most of the picture. */
const START_STRENGTH = { fix: 0.75, redo: 0.4 } as const;

const TITLE: Record<Step, string> = { view: 'Picture', fix: 'Fix a part', redo: 'Redo' };

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

/** The bar's fill, in percent: the steps done while sampling (never past all of it), all of it after, none before. */
export function progressShare(progress: RedrawProgress | null): number {
  if (progress?.stage === 'decoding' || progress?.stage === 'finishing') return 100;
  if (progress?.stage === 'sampling' && progress.step && progress.total) return Math.min(100, Math.round((progress.step / progress.total) * 100));
  return 0;
}

interface PictureWorkProps {
  url: string;
  size: string;
  picture: AttachmentInfo;
  /** What the picture was drawn from: the prompt its tool call gave. */
  prompt: string;
  actions: PictureActions;
  from: RefObject<HTMLElement | null>;
  onClose: () => void;
}

export const PictureWork: FC<PictureWorkProps> = ({ url, size, picture, prompt, actions, from, onClose }) => {
  const { conversationId, models, picked, onKept } = actions;
  const [step, setStep] = useState<Step>('view');
  const brush = useBrush(picture);
  const [words, setWords] = useState(prompt);
  const [strength, setStrength] = useState<number>(START_STRENGTH.fix);
  // A choice the machine no longer lists, or that lacks a file, is not one
  // the menu can show: the machine's own choice stands in for it.
  const [model, setModel] = useState<number | null>(() =>
    models.some((row) => row.id === picked && row.complete) ? picked : null,
  );
  const [comparing, setComparing] = useState(false);
  // The question asked on closing, open now: a sheet that goes answers it no.
  const asking = useRef(false);
  useEffect(
    () => () => {
      if (asking.current) dismissConfirm();
    },
    [],
  );

  const source = useMemo<RedrawSource>(
    () => ({
      id: picture.id,
      width: picture.width,
      height: picture.height,
      blob: () => getTransport().fetchAttachmentBlob('this', picture.id),
    }),
    [picture.id, picture.width, picture.height],
  );
  // Kept: the sheet closes whether or not the chat could be shown again.
  const { state, start, stop, discard, keep } = usePictureRedraw(source, conversationId, async () => {
    try {
      await onKept();
    } finally {
      onClose();
    }
  });
  const drawn = state.phase === 'drawn' || state.phase === 'keeping' ? state.picture : null;
  const drawnUrl = useBlobUrl(drawn);
  const editing = state.phase === 'idle' || state.phase === 'failed';
  const usable = models.filter((row) => row.complete);

  const choose = (next: 'fix' | 'redo') => {
    setStep(next);
    setStrength(START_STRENGTH[next]);
  };
  const ask = () => start({ prompt: words.trim(), strength, strokes: step === 'fix' ? brush.strokes : [], model });
  const ready = words.trim().length > 0 && (step === 'redo' || marksSomething(brush.strokes));
  const blocked = !words.trim() ? 'Say what the picture should show.' : !ready ? 'Brush over the part to fix first.' : undefined;

  // A render under way is stopped by closing; a picture drawn and not kept is asked about first.
  const mayClose: MayClose = () => {
    if (state.phase === 'keeping') return false;
    if (state.phase !== 'drawn') return true;
    asking.current = true;
    return confirm({
      title: 'Discard the new picture?',
      description: 'It has not been kept. The picture it came from stays in the chat.',
      confirmLabel: 'Discard',
    }).finally(() => {
      asking.current = false;
    });
  };

  const hold = (on: boolean) => () => setComparing(on);
  const footer = drawn ? (
    <>
      <Button
        variant="ghost"
        aria-pressed={comparing}
        onPointerDown={hold(true)}
        onPointerUp={hold(false)}
        onPointerLeave={hold(false)}
        onPointerCancel={hold(false)}
        onKeyDown={(event) => (event.key === ' ' || event.key === 'Enter') && setComparing(true)}
        onKeyUp={hold(false)}
        onBlur={hold(false)}
      >
        Hold to see the original
      </Button>
      <span className="flex-1" />
      <Button variant="ghost" onClick={discard} disabled={state.phase === 'keeping'}>Discard</Button>
      <Button variant="secondary" onClick={ask} disabled={state.phase === 'keeping'}>Try again</Button>
      <Button onClick={() => void keep()} disabled={state.phase === 'keeping'}>
        {state.phase === 'keeping' ? 'Keeping…' : 'Keep'}
      </Button>
    </>
  ) : state.phase === 'drawing' ? (
    <>
      <div className="flex min-w-0 flex-1 flex-col gap-xs">
        <span role="status" className="text-sm tabular-nums text-text-secondary">{progressLine(state.progress)}</span>
        <span role="progressbar" aria-label="Drawing" aria-valuenow={progressShare(state.progress)} aria-valuemin={0} aria-valuemax={100} className="block h-[2px] w-full bg-border">
          <span className="block h-full bg-text-secondary" style={{ width: `${progressShare(state.progress)}%` }} />
        </span>
      </div>
      <Button variant="secondary" onClick={stop}>Stop</Button>
    </>
  ) : step === 'view' ? (
    <>
      <span className="flex-1" />
      <Button variant="secondary" onClick={() => choose('fix')}>Fix a part</Button>
      <Button variant="secondary" onClick={() => choose('redo')}>Redo</Button>
    </>
  ) : (
    <>
      {/* In words: a disabled button shows no title, and touch has none. */}
      <span className="flex-1 text-sm text-text-muted">{blocked}</span>
      <Button variant="ghost" onClick={() => setStep('view')}>Cancel</Button>
      <Button onClick={ask} disabled={!ready}>{step === 'fix' ? 'Fix this part' : 'Draw it again'}</Button>
    </>
  );

  const frame = state.phase === 'drawing' ? state.progress?.frame : undefined;
  const result = drawn !== null && !comparing;
  const failure = state.phase === 'failed' ? state.message : state.phase === 'drawn' ? state.problem : undefined;

  const side = editing && step !== 'view' && (
    <>
      {step === 'fix' && <BrushRow picture={picture} brush={brush} />}
      <label className="flex flex-col gap-xs">
        {step === 'fix' ? 'What the brushed part should show' : 'What the picture should show'}
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
    </>
  );

  return (
    <SheetFrame
      title={drawn ? 'Keep this picture?' : TITLE[step]}
      size={size}
      from={from}
      onClose={onClose}
      mayClose={mayClose}
      side={side || undefined}
      footer={footer}
    >
      <PictureBox width={picture.width} height={picture.height}>
        <img
          src={result && drawnUrl ? drawnUrl : url}
          alt={result ? 'The picture drawn again' : `Image, ${size}`}
          className="block h-full w-full object-contain"
          draggable={false}
        />
        {frame && <img src={`data:image/png;base64,${frame}`} alt="The picture so far" className="absolute inset-0 h-full w-full object-contain opacity-90" />}
        {editing && step === 'fix' && <BrushCanvas picture={picture} brush={brush} />}
      </PictureBox>
      {failure && (
        <p role="alert" className="m-0 flex items-baseline gap-xs text-sm text-text">
          <StatusMark kind="failed" label="Failed" />
          {failure}
        </p>
      )}
    </SheetFrame>
  );
};
