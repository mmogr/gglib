import { useState, useCallback, useRef, useEffect, useMemo } from "react";
import { browseHfModels } from "../../../services/clients/huggingface";
import {
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfSortField,
} from "../../../types";
import {
  parseModelSearchIntent,
  getButtonTextForIntent,
  getButtonVariantForIntent,
  ModelSearchIntent,
} from "../../../utils/modelSearchParser";
import { queueDownload } from "../../../services/clients/downloads";
import { useToastContext } from "../../../contexts/ToastContext";
import { useDebounce } from "../../../hooks/useDebounce";

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

  // Results state
  models: HfModelSummary[];
  hasMore: boolean;
  currentPage: number;

  // Loading/error state
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
  searchError: string | null;

  // Search intent
  searchIntent: ModelSearchIntent;
  buttonText: string;
  buttonVariant: "default" | "primary" | "accent";

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

  // Results state
  const [models, setModels] = useState<HfModelSummary[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [currentPage, setCurrentPage] = useState(0);

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

  // Clear search error on query change
  useEffect(() => {
    if (searchError) {
      setSearchError(null);
    }
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
    // Trigger a new search with the updated sort
    setCurrentPage(0);
  }, []);

  // Build search request
  const buildSearchRequest = useCallback(
    (page: number): HfSearchRequest => ({
      query: searchQuery.trim() || null,
      min_params_b: minParams ? parseFloat(minParams) : null,
      max_params_b: maxParams ? parseFloat(maxParams) : null,
      page,
      limit: 30,
      sort_by: sortBy,
      sort_ascending: sortAscending,
    }),
    [searchQuery, minParams, maxParams, sortBy, sortAscending]
  );

  // Perform search
  const performSearch = useCallback(
    async (page: number, append: boolean = false) => {
      if (page === 0) {
        setLoading(true);
      } else {
        setLoadingMore(true);
      }
      setError(null);

      try {
        const request = buildSearchRequest(page);
        const response: HfSearchResponse = await browseHfModels(request);

        if (append) {
          setModels((prev) => [...prev, ...response.models]);
        } else {
          setModels(response.models);
        }
        setHasMore(response.has_more);
        setCurrentPage(response.page);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to search models"
        );
      } finally {
        setLoading(false);
        setLoadingMore(false);
      }
    },
    [buildSearchRequest]
  );

  // Handle search button click - now handles all intents
  const handleSearch = useCallback(() => {
    setCurrentPage(0);
    performSearch(0, false);
  }, [performSearch]);

  // Handle direct download for exact repo:quant pattern
  const handleDirectDownload = useCallback(
    async (repo: string, quant: string) => {
      try {
        setLoading(true);
        await queueDownload({ modelId: repo, quantization: quant });
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

  // Handle view model for exact repo pattern - fetch and select
  const handleViewRepo = useCallback(
    async (repo: string) => {
      try {
        setLoading(true);
        // Search for the exact repo to get model summary
        const request: HfSearchRequest = {
          query: repo,
          min_params_b: null,
          max_params_b: null,
          page: 0,
          limit: 10,
          sort_by: "downloads",
          sort_ascending: false,
        };
        const response = await browseHfModels(request);
        // Find exact match
        const exactMatch = response.models.find((m) => m.id === repo);
        if (exactMatch) {
          onSelectModel?.(exactMatch);
          setSearchQuery(""); // Clear search after selecting
        } else {
          setSearchError(`Model "${repo}" not found on HuggingFace`);
          showToast(`Model "${repo}" not found`, "error");
        }
      } catch (err) {
        const message =
          err instanceof Error ? err.message : "Failed to fetch model";
        setSearchError(message);
        showToast(message, "error");
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

  // Handle load more
  const handleLoadMore = useCallback(() => {
    performSearch(currentPage + 1, true);
  }, [performSearch, currentPage]);

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
  const buttonVariant = getButtonVariantForIntent(searchIntent);

  // Auto-search on debounced query change (after initial mount)
  useEffect(() => {
    if (isInitialMount.current) {
      isInitialMount.current = false;
      // Perform initial search
      performSearch(0, false);
      return;
    }

    // Don't auto-search if query is empty (user cleared input)
    // They can manually click Search to get all results
  }, [debouncedQuery, performSearch]);

  // Auto-search when sort changes
  useEffect(() => {
    if (!isInitialMount.current) {
      performSearch(0, false);
    }
  }, [sortBy, sortAscending, performSearch]);

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

    // Results state
    models,
    hasMore,
    currentPage,

    // Loading/error state
    loading,
    loadingMore,
    error,
    searchError,

    // Search intent
    searchIntent,
    buttonText,
    buttonVariant,

    // Actions
    handleSearch,
    handleSearchAction,
    handleLoadMore,
    handleKeyDown,
  };
}
