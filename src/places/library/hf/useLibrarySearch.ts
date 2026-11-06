/**
 * The one search: the field's text, which filters the library as it is
 * typed, and what Enter made of it (`enterAction`). Enter searches Hugging
 * Face, opens an exact `user/repo` in the inspector, or offers a
 * `user/repo:QUANT` download as the Hugging Face group's first row.
 * What Enter did stands while the field still says what was entered. A
 * repo's answer counts only while nothing has happened since its Enter: an
 * edit, another Enter, or a pick of the library (`cancelOpen`).
 *
 * The field's first focus puts the search in Hugging Face's scope: with it
 * empty, the Hugging Face group shows the model suggested for this machine.
 *
 * @module places/library/hf/useLibrarySearch
 */

import { useCallback, useRef, useState } from 'react';
import type { HfModelSummary } from '../../../types';
import { getTransport } from '../../../services/transport';
import { formatError } from '../../../utils/errors';
import { parseModelSearchIntent } from '../../../utils/modelSearchParser';
import { useHuggingFaceSearch, type HuggingFaceSearch } from './useHuggingFaceSearch';

export type EnterAction =
  | { kind: 'search'; query: string }
  | { kind: 'open'; repo: string }
  | { kind: 'download'; repo: string; quant: string };

/** What Enter does with `text`, or `null` for an empty field. */
export function enterAction(text: string): EnterAction | null {
  const intent = parseModelSearchIntent(text);
  switch (intent.kind) {
    case 'download':
      return { kind: 'download', repo: intent.repo, quant: intent.quant };
    case 'repo':
      return { kind: 'open', repo: intent.repo };
    case 'url':
      if (intent.repo && intent.quant) return { kind: 'download', repo: intent.repo, quant: intent.quant };
      if (intent.repo) return { kind: 'open', repo: intent.repo };
      return { kind: 'search', query: intent.url };
    case 'search':
      return intent.query ? { kind: 'search', query: intent.query } : null;
  }
}

export interface LibrarySearch {
  query: string;
  setQuery: (query: string) => void;
  /** The field took focus: the search is in Hugging Face's scope. */
  onFocus: () => void;
  /** Act on the field's text as Enter does. */
  enter: () => void;
  /** What the Hugging Face group shows, or `null` when it is not drawn. */
  shown: { kind: 'suggestion' } | EnterAction | null;
  hf: HuggingFaceSearch;
  /** Why the repo Enter named could not be opened, as the daemon said it. */
  openError: string | null;
  /** Drop the answer to a repo Enter named, if it is still to come. */
  cancelOpen: () => void;
}

export function useLibrarySearch(onOpen: (model: HfModelSummary) => void): LibrarySearch {
  const [query, setQuery] = useState('');
  const [scoped, setScoped] = useState(false);
  const [entered, setEntered] = useState<{ text: string; action: EnterAction } | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  const hf = useHuggingFaceSearch();
  // The latest open's number: an answer to any earlier one is dropped.
  const opening = useRef(0);
  const cancelOpen = useCallback(() => {
    opening.current += 1;
  }, []);
  const edit = useCallback((next: string) => {
    opening.current += 1;
    setQuery(next);
  }, []);

  const text = query.trim();

  const enter = useCallback(() => {
    const action = enterAction(text);
    if (action === null) return;
    const seq = ++opening.current;
    setEntered({ text, action });
    setOpenError(null);
    if (action.kind === 'search') {
      hf.search(action.query);
      return;
    }
    hf.clear();
    if (action.kind === 'open') {
      getTransport()
        .getHfModelSummary(action.repo)
        .then((model) => {
          if (seq !== opening.current) return;
          onOpen(model);
          setQuery('');
        })
        .catch((err: unknown) => {
          if (seq === opening.current) setOpenError(formatError(err));
        });
    }
  }, [text, hf, onOpen]);

  const shown =
    text === '' ? (scoped ? ({ kind: 'suggestion' } as const) : null) : entered?.text === text ? entered.action : null;

  return { query, setQuery: edit, onFocus: () => setScoped(true), enter, shown, hf, openError, cancelOpen };
}
