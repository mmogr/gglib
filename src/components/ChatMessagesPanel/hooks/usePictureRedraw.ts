import { useCallback, useEffect, useRef, useState } from 'react';
import { getTransport } from '../../../services/transport';
import type { RedrawProgress } from '../../../services/transport/api/images';
import { formatError } from '../../../utils/errors';
import { base64Of, maskPng, pngOf, type MaskStroke } from '../../../utils/pictureMask';

/** What a person asks of a redraw. */
export interface RedrawAsk {
  prompt: string;
  /** How far from the picture the result may go, from 0.05 to 1. */
  strength: number;
  /** What was brushed; none to draw the whole picture again. */
  strokes: readonly MaskStroke[];
  /** The image model to draw with, by id; `null` for the machine's own choice. */
  model: number | null;
}

/** Where a redraw is. */
export type RedrawState =
  | { phase: 'idle' }
  | { phase: 'drawing'; progress: RedrawProgress | null }
  | { phase: 'drawn'; picture: Blob; /** Why the last Keep did not keep it. */ problem?: string }
  | { phase: 'keeping'; picture: Blob }
  | { phase: 'failed'; message: string };

/** The picture a redraw starts from: its bytes, its size, and its id in the store. */
export interface RedrawSource {
  id: string;
  width: number;
  height: number;
  /** Its bytes, read from the store. */
  blob: () => Promise<Blob>;
}

export interface PictureRedraw {
  state: RedrawState;
  /** Draw the picture again as `ask` says. */
  start: (ask: RedrawAsk) => void;
  /** Stop the render being drawn; the machine stops one nobody reads. */
  stop: () => void;
  /** Throw the drawn picture away and go back to asking. */
  discard: () => void;
  /** Store the drawn picture and keep it beside the one it was made from. */
  keep: () => Promise<void>;
}

/**
 * One picture drawn again on this machine: the render followed from the
 * request to the picture, which is then kept in the chat or thrown away.
 * `onKept` is told once the chat holds it. A Keep that fails leaves the
 * picture drawn, to be kept again. A render still going when the component
 * goes is stopped.
 */
export function usePictureRedraw(
  source: RedrawSource,
  conversationId: number,
  onKept: () => void | Promise<void>,
): PictureRedraw {
  const [state, setState] = useState<RedrawState>({ phase: 'idle' });
  const stopRender = useRef<(() => void) | null>(null);
  // Which request the page is waiting on: an answer to an older one is dropped.
  const asked = useRef(0);

  const stop = useCallback(() => {
    asked.current += 1;
    stopRender.current?.();
    stopRender.current = null;
    setState({ phase: 'idle' });
  }, []);

  useEffect(
    () => () => {
      asked.current += 1;
      stopRender.current?.();
    },
    [],
  );

  const start = useCallback(
    (ask: RedrawAsk) => {
      stopRender.current?.();
      const mine = (asked.current += 1);
      const current = () => asked.current === mine;
      setState({ phase: 'drawing', progress: null });
      void (async () => {
        try {
          const image = await base64Of(await source.blob());
          const mask =
            ask.strokes.length > 0
              ? await base64Of(await maskPng(ask.strokes, source.width, source.height))
              : undefined;
          if (!current()) return;
          stopRender.current = getTransport().redrawPicture(
            {
              prompt: ask.prompt,
              image,
              strength: ask.strength,
              ...(mask !== undefined && { mask }),
              ...(ask.model !== null && { model: ask.model }),
            },
            {
              onProgress: (progress) => {
                // A report with no frame keeps the last frame shown.
                if (current()) {
                  setState((was) => ({
                    phase: 'drawing',
                    progress: {
                      ...progress,
                      ...(!progress.frame && was.phase === 'drawing' && was.progress?.frame && { frame: was.progress.frame }),
                    },
                  }));
                }
              },
              onDrawn: (png) => {
                if (current()) setState({ phase: 'drawn', picture: pngOf(png) });
              },
              onFailed: (message) => {
                if (current()) setState({ phase: 'failed', message });
              },
            },
          );
        } catch (error) {
          if (current()) setState({ phase: 'failed', message: formatError(error) });
        }
      })();
    },
    [source],
  );

  const discard = useCallback(() => {
    asked.current += 1;
    setState({ phase: 'idle' });
  }, []);

  const keep = useCallback(async () => {
    if (state.phase !== 'drawn') return;
    const { picture } = state;
    setState({ phase: 'keeping', picture });
    try {
      const stored = await getTransport().uploadAttachment('this', picture);
      await getTransport().keepPicture(conversationId, { from: source.id, image: stored.id });
    } catch (error) {
      // The picture is still the page's: Keep can be pressed again.
      setState({ phase: 'drawn', picture, problem: `The picture was drawn but could not be kept: ${formatError(error)}` });
      return;
    }
    // It is kept. Showing it is the chat's to do, and a chat that cannot be
    // read again just now shows it when it next is.
    try {
      await onKept();
    } catch {
      /* kept all the same */
    }
  }, [state, conversationId, source.id, onKept]);

  return { state, start, stop, discard, keep };
}
