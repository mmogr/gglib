/**
 * The library's Hugging Face search: what was searched for, the kind and
 * sort it asks with, and the pages the Hub has given back.
 *
 * Nothing is asked until `search` is called (Enter, or the hint row); a
 * change of kind or sort then searches again from the first page. "More
 * results" carries on from the cursor the last page ended with. An answer
 * that a newer request has overtaken is dropped, so a slow search never
 * lands over the one asked after it.
 *
 * A request Hugging Face rate limits waits until the limit lifts and is
 * made again (`whenHubAllows`); a newer request, a clear or leaving the
 * library stops the wait. A first page that waits or fails has no rows; a
 * "More results" that does keeps the rows already listed.
 *
 * @module places/library/hf/useHuggingFaceSearch
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { HfModelKind, HfModelSummary, HfSearchRequest, HfSortField } from '../../../types';
import { getTransport } from '../../../services/transport';
import { whenHubAllows } from '../../../services/transport/hubLimit';
import { formatError, isAbortError } from '../../../utils/errors';

/** Each sort and the direction it starts in. */
export const HF_SORTS: { value: HfSortField; label: string; ascending: boolean }[] = [
  { value: 'downloads', label: 'Downloads', ascending: false },
  { value: 'likes', label: 'Likes', ascending: false },
  { value: 'modified', label: 'Updated', ascending: false },
  { value: 'created', label: 'Created', ascending: false },
  { value: 'id', label: 'Alphabetical', ascending: true },
];

export interface HfSort {
  by: HfSortField;
  ascending: boolean;
}

export interface HuggingFaceSearch {
  /** The text last searched for, or `null` before the first search. */
  searched: string | null;
  kind: HfModelKind;
  setKind: (kind: HfModelKind) => void;
  sort: HfSort;
  setSort: (sort: HfSort) => void;
  models: HfModelSummary[];
  hasMore: boolean;
  loading: boolean;
  loadingMore: boolean;
  /** Why the last request failed, as the daemon said it. */
  error: string | null;
  /** When the Hub's limit lifts (epoch ms) while the last request waits for it, else `null`. */
  liftsAt: number | null;
  search: (query: string) => void;
  loadMore: () => void;
  /** Forget the search and drop any answer still to come. */
  clear: () => void;
}

export function useHuggingFaceSearch(): HuggingFaceSearch {
  const [searched, setSearched] = useState<string | null>(null);
  const [kind, setKind] = useState<HfModelKind>('chat');
  const [sort, setSort] = useState<HfSort>({ by: 'downloads', ascending: false });
  const [models, setModels] = useState<HfModelSummary[]>([]);
  // Where the next page starts, as the Hub gave it; null on the last page.
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [liftsAt, setLiftsAt] = useState<number | null>(null);
  // The latest request's number: an answer to any earlier one is dropped.
  const latest = useRef(0);
  // Stops the latest request's wait for the Hub's limit.
  const waiting = useRef<AbortController | null>(null);
  const stopWaiting = useCallback(() => {
    waiting.current?.abort();
    waiting.current = null;
  }, []);
  // Leaving the library stops a wait still out.
  useEffect(() => {
    return stopWaiting;
  }, [stopWaiting]);

  const fetchPage = useCallback(async (request: HfSearchRequest) => {
    const seq = ++latest.current;
    stopWaiting();
    const stop = new AbortController();
    waiting.current = stop;
    const append = request.cursor !== null;
    const setBusy = append ? setLoadingMore : setLoading;
    setBusy(true);
    setError(null);
    setLiftsAt(null);
    // A first page refused has no rows, so none of an earlier search show above its line.
    const dropRows = () => {
      if (append) return;
      setModels([]);
      setNextCursor(null);
    };
    try {
      const response = await whenHubAllows(() => getTransport().browseHfModels(request), {
        signal: stop.signal,
        onWaiting: (at) => {
          if (seq !== latest.current) return;
          if (at !== null) dropRows();
          setLiftsAt(at);
          setBusy(at === null);
        },
      });
      if (seq !== latest.current) return;
      setModels((prev) => (append ? [...prev, ...response.models] : response.models));
      setNextCursor(response.next_cursor);
    } catch (err) {
      // A wait stopped is a newer request, a clear or the library left.
      if (seq !== latest.current || isAbortError(err)) return;
      dropRows();
      setLiftsAt(null);
      setError(formatError(err));
    } finally {
      if (seq === latest.current) {
        setLoading(false);
        setLoadingMore(false);
      }
    }
  }, [stopWaiting]);

  const request = useCallback(
    (query: string, cursor: string | null): HfSearchRequest => ({
      query: query || null,
      min_params_b: null,
      max_params_b: null,
      cursor,
      limit: 100,
      sort_by: sort.by,
      sort_ascending: sort.ascending,
      kind,
    }),
    [sort, kind],
  );

  // A new search, kind or sort asks for the first page again.
  useEffect(() => {
    if (searched !== null) void fetchPage(request(searched, null));
  }, [searched, request, fetchPage]);

  const search = useCallback(
    (query: string) => {
      if (query === searched) void fetchPage(request(query, null));
      else setSearched(query);
    },
    [searched, request, fetchPage],
  );

  const loadMore = useCallback(() => {
    if (searched !== null && nextCursor !== null) void fetchPage(request(searched, nextCursor));
  }, [searched, nextCursor, request, fetchPage]);

  const clear = useCallback(() => {
    latest.current += 1;
    stopWaiting();
    setSearched(null);
    setModels([]);
    setNextCursor(null);
    setLoading(false);
    setLoadingMore(false);
    setError(null);
    setLiftsAt(null);
  }, [stopWaiting]);

  return {
    searched,
    kind,
    setKind,
    sort,
    setSort,
    models,
    hasMore: nextCursor !== null,
    loading,
    loadingMore,
    error,
    liftsAt,
    search,
    loadMore,
    clear,
  };
}
