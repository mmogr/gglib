import { useState, useCallback, useRef, useEffect, useMemo } from "react";
import {
  HfModelKind,
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfSortField,
} from "../../../types";
import {
  parseModelSearchIntent,
  getButtonTextForIntent,
  ModelSearchIntent,
} from "../../../utils/modelSearchParser";
import { useToastContext } from "../../../contexts/ToastContext";
import { useDebounce } from "../../../hooks/useDebounce";
import { getTransport } from '../../../services/transport';

// Sort options configuration
interface SortOption {
  value: HfSortField;
  label: string;
  defaultAscending: boolean;
}

export const SORT_OPTIONS: SortOption[] = [
  { value: "downloads", label: "Downloads", defaultAscending: false },
  { value: "likes", label: "Likes", defaultAscending: false },
  { value: "modified", label: "Recently Updated", defaultAscending: false },
  { value: "created", label: "Recently Created", defaultAscending: false },
  { value: "id", label: "Alphabetical", defaultAscending: true },
];

export interface UseHuggingFaceSearchOptions {
  /** Callback when a model is selected (clicked) for preview */
  onSelectModel?: (model: HfModelSummary | null) => void;
}

export interface UseHuggingFaceSearchReturn {
  // Search input state
  searchQuery: string;
  setSearchQuery: (query: string) => void;
  minParams: string;
  setMinParams: (value: string) => void;
  maxParams: string;
  setMaxParams: (value: string) => void;
  sortBy: HfSortField;
  sortAscending: boolean;
  handleSortChange: (newSortBy: HfSortField) => void;
  setSortAscending: (ascending: boolean) => void;
  /** Models that chat or models that draw; a change searches again. */
  kind: HfModelKind;
  setKind: (kind: HfModelKind) => void;

  // Results state
  models: HfModelSummary[];
  hasMore: boolean;

  // Loading/error state
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
  searchError: string | null;

  // Search intent
  searchIntent: ModelSearchIntent;
  buttonText: string;

  // Actions
  handleSearch: () => void;
  handleSearchAction: () => void;
  handleLoadMore: () => void;
  handleKeyDown: (e: React.KeyboardEvent) => void;
}

/**
 * Hook for managing HuggingFace model search state and logic.
 * 
 * Encapsulates all search-related state, API calls, and intent parsing.
 */
export function useHuggingFaceSearch(
  options: UseHuggingFaceSearchOptions = {}
): UseHuggingFaceSearchReturn {
  const { onSelectModel } = options;

  // Search state
  const [searchQuery, setSearchQuery] = useState("");
  const [minParams, setMinParams] = useState("");
  const [maxParams, setMaxParams] = useState("");
  const [sortBy, setSortBy] = useState<HfSortField>("downloads");
  const [sortAscending, setSortAscending] = useState(false);
  const [kind, setKind] = useState<HfModelKind>("chat");

  // Results state
  const [models, setModels] = useState<HfModelSummary[]>([]);
  // Where the next page starts, as the Hub gave it; null on the last page.
  const [nextCursor, setNextCursor] = useState<string | null>(null);

  // Loading/error state
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchError, setSearchError] = useState<string | null>(null);

  // Toast for notifications
  const { showToast } = useToastContext();

  // Compute search intent from current query
  const searchIntent = useMemo(
    () => parseModelSearchIntent(searchQuery),
    [searchQuery]
  );

  // Clear search error on query change. Unconditional set: React bails out
  // of re-rendering when the value is already null, so no guard (and thus no
  // searchError dependency) is needed.
  useEffect(() => {
    setSearchError(null);
  }, [searchQuery]);

  // Debounced search
  const debouncedQuery = useDebounce(searchQuery, 300);

  // Ref to track if we should fetch on debounced change
  const isInitialMount = useRef(true);

  // Handle sort change
  const handleSortChange = useCallback((newSortBy: HfSortField) => {
    const sortOption = SORT_OPTIONS.find((opt) => opt.value === newSortBy);
    setSortBy(newSortBy);
    // Set the default direction for this sort option
    if (sortOption) {
      setSortAscending(sortOption.defaultAscending);
    }
  }, []);

  // Build the request for a new search's first page. A page is as many
  // repositories as the Hub gives at once, so a filter that drops most of
  // them still leaves something to show.
  const buildSearchRequest = useCallback(
    (): HfSearchRequest => ({
      query: searchQuery.trim() || null,
      min_params_b: minParams ? parseFloat(minParams) : null,
      max_params_b: maxParams ? parseFloat(maxParams) : null,
      cursor: null,
      limit: 100,
      sort_by: sortBy,
      sort_ascending: sortAscending,
      kind,
    }),
    [searchQuery, minParams, maxParams, sortBy, sortAscending, kind]
  );

  // Fetch one page: a new search's first, or the page after the shown ones
  // when `request` carries their cursor.
  const fetchPage = useCallback(
    async (request: HfSearchRequest) => {
      const append = request.cursor !== null;
      if (append) {
        setLoadingMore(true);
      } else {
        setLoading(true);
      }
      setError(null);

      try {
        const response: HfSearchResponse = await getTransport().browseHfModels(request);

        if (append) {
          setModels((prev) => [...prev, ...response.models]);
        } else {
          setModels(response.models);
        }
        setNextCursor(response.next_cursor);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to search models"
        );
      } finally {
        setLoading(false);
        setLoadingMore(false);
      }
    },
    []
  );

  // Run a new search from the inputs, from its first page.
  const performSearch = useCallback(
    () => fetchPage(buildSearchRequest()),
    [fetchPage, buildSearchRequest]
  );

  // Handle search button click - now handles all intents
  const handleSearch = useCallback(() => {
    performSearch();
  }, [performSearch]);

  // Handle direct download for exact repo:quant pattern
  const handleDirectDownload = useCallback(
    async (repo: string, quant: string) => {
      try {
        setLoading(true);
        await getTransport().queueDownload({ modelId: repo, quantization: quant });
        showToast(`Download started: ${repo} (${quant})`, "success");
        setSearchQuery(""); // Clear search after successful queue
      } catch (err) {
        const message =
          err instanceof Error ? err.message : "Failed to start download";
        setSearchError(message);
        showToast(message, "error");
      } finally {
        setLoading(false);
      }
    },
    [showToast]
  );

  // Handle view model for exact repo pattern - fetch directly by ID
  const handleViewRepo = useCallback(
    async (repo: string) => {
      try {
        setLoading(true);
        // Fetch model directly by repo ID (not search)
        const model = await getTransport().getHfModelSummary(repo);
        onSelectModel?.(model);
        setSearchQuery(""); // Clear search after selecting
      } catch (err) {
        // Extract user-friendly error message
        const rawMessage =
          err instanceof Error ? err.message : "Failed to fetch model";
        
        // Check for specific error cases
        let userMessage: string;
        if (rawMessage.includes("no GGUF files")) {
          userMessage = `Model "${repo}" exists but contains no GGUF files`;
        } else if (rawMessage.includes("not found") || rawMessage.includes("404")) {
          userMessage = `Model "${repo}" not found on HuggingFace`;
        } else {
          userMessage = `Failed to load model: ${rawMessage}`;
        }
        
        setSearchError(userMessage);
        showToast(userMessage, "error");
      } finally {
        setLoading(false);
      }
    },
    [onSelectModel, showToast]
  );

  // Unified action handler based on current intent
  const handleSearchAction = useCallback(() => {
    switch (searchIntent.kind) {
      case "download":
        handleDirectDownload(searchIntent.repo, searchIntent.quant);
        break;
      case "repo":
        handleViewRepo(searchIntent.repo);
        break;
      case "url":
        if (searchIntent.quant && searchIntent.repo) {
          handleDirectDownload(searchIntent.repo, searchIntent.quant);
        } else if (searchIntent.repo) {
          handleViewRepo(searchIntent.repo);
        } else {
          handleSearch();
        }
        break;
      case "search":
      default:
        handleSearch();
        break;
    }
  }, [searchIntent, handleDirectDownload, handleViewRepo, handleSearch]);

  // Handle load more: the search, carried on from the last page's cursor.
  // Any change to the inputs searches again from the first page, so the
  // cursor always belongs to the search the inputs describe.
  const handleLoadMore = useCallback(() => {
    if (nextCursor !== null) {
      fetchPage({ ...buildSearchRequest(), cursor: nextCursor });
    }
  }, [fetchPage, buildSearchRequest, nextCursor]);

  // Handle enter key in search input - uses unified action handler
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        handleSearchAction();
      }
    },
    [handleSearchAction]
  );

  // Get button text and variant based on intent
  const buttonText = loading
    ? "Loading..."
    : getButtonTextForIntent(searchIntent);

  // Auto-search on debounced query change (after initial mount)
  useEffect(() => {
    if (isInitialMount.current) {
      isInitialMount.current = false;
      // Perform initial search
      performSearch();
      return;
    }

    // Don't auto-search if query is empty (user cleared input)
    // They can manually click Search to get all results
  }, [debouncedQuery, performSearch]);

  // Auto-search when the sort or the kind changes
  useEffect(() => {
    if (!isInitialMount.current) {
      performSearch();
    }
  }, [sortBy, sortAscending, kind, performSearch]);

  return {
    // Search input state
    searchQuery,
    setSearchQuery,
    minParams,
    setMinParams,
    maxParams,
    setMaxParams,
    sortBy,
    sortAscending,
    handleSortChange,
    setSortAscending,
    kind,
    setKind,

    // Results state
    models,
    hasMore: nextCursor !== null,

    // Loading/error state
    loading,
    loadingMore,
    error,
    searchError,

    // Search intent
    searchIntent,
    buttonText,

    // Actions
    handleSearch,
    handleSearchAction,
    handleLoadMore,
    handleKeyDown,
  };
}
