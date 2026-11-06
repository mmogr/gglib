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
 * @module places/library/hf/useHuggingFaceSearch
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { HfModelKind, HfModelSummary, HfSearchRequest, HfSortField } from '../../../types';
import { getTransport } from '../../../services/transport';
import { formatError } from '../../../utils/errors';

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
  // The latest request's number: an answer to any earlier one is dropped.
  const latest = useRef(0);

  const fetchPage = useCallback(async (request: HfSearchRequest) => {
    const seq = ++latest.current;
    const append = request.cursor !== null;
    if (append) setLoadingMore(true);
    else setLoading(true);
    setError(null);
    try {
      const response = await getTransport().browseHfModels(request);
      if (seq !== latest.current) return;
      setModels((prev) => (append ? [...prev, ...response.models] : response.models));
      setNextCursor(response.next_cursor);
    } catch (err) {
      if (seq === latest.current) setError(formatError(err));
    } finally {
      if (seq === latest.current) {
        setLoading(false);
        setLoadingMore(false);
      }
    }
  }, []);

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
    setSearched(null);
    setModels([]);
    setNextCursor(null);
    setLoading(false);
    setLoadingMore(false);
    setError(null);
  }, []);

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
    search,
    loadMore,
    clear,
  };
}
