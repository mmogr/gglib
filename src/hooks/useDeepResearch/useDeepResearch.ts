/**
 * useDeepResearch Hook
 *
 * Integration hook that wraps runResearchLoop with:
 * - Debounced incremental persistence ("Save-As-You-Go")
 * - AbortController for emergency stop
 * - Connection to gglib's tool registry and LLM proxy
 * - UI state synchronization
 *
 * @module hooks/useDeepResearch/useDeepResearch
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import type { ResearchState, ModelRouting } from './types';
import { createInitialState, createDefaultRouting } from './types';
import {
  runResearchLoop,
  type ToolDefinition,
  type ToolResult,
  type LLMResponse,
} from './runResearchLoop';
import type { TurnMessage } from './buildTurnMessages';
import { getToolRegistry } from '../../services/tools';

// =============================================================================
// Configuration
// =============================================================================

/** Debounce interval for persistence (ms) */
const PERSIST_DEBOUNCE_MS = 2000;

/** Default max steps for research */
const DEFAULT_MAX_STEPS = 30;

// =============================================================================
// Types
// =============================================================================

export interface UseDeepResearchOptions {
  /** Server port for LLM calls */
  serverPort: number;
  /** Conversation ID for persistence */
  conversationId?: number;
  /** Base system prompt (from conversation) */
  systemPrompt?: string;
  /** Maximum research steps */
  maxSteps?: number;
  /** Called when state changes (for UI updates) */
  onStateChange?: (state: ResearchState) => void;
  /** Called to persist state to database */
  onPersist?: (state: ResearchState) => Promise<void>;
  /** Called on error */
  onError?: (error: Error) => void;
}

export interface UseDeepResearchReturn {
  /** Current research state (null if not running) */
  state: ResearchState | null;
  /** Whether research is currently running */
  isRunning: boolean;
  /** Start a new research session */
  startResearch: (query: string, messageId: string) => Promise<void>;
  /** Stop the current research session (graceful) */
  stopResearch: () => void;
  /** Reset state (for cleanup) */
  resetState: () => void;
}

// =============================================================================
// Tool Execution Adapter
// =============================================================================

/**
 * Create a tool executor that uses gglib's tool registry.
 */
function createToolExecutor(): (
  name: string,
  args: Record<string, unknown>
) => Promise<ToolResult> {
  return async (name, args) => {
    const registry = getToolRegistry();
    return registry.execute(name, args);
  };
}

// =============================================================================
// LLM Caller Adapter
// =============================================================================

/**
 * Create an LLM caller that uses gglib's proxy server.
 */
function createLLMCaller(): (
  messages: TurnMessage[],
  options: {
    tools?: ToolDefinition[];
    endpoint: { port: number; modelId?: number };
    abortSignal?: AbortSignal;
  }
) => Promise<LLMResponse> {
  return async (messages, { tools, endpoint, abortSignal }) => {
    const url = `http://127.0.0.1:${endpoint.port}/v1/chat/completions`;

    const body: Record<string, unknown> = {
      messages,
      stream: false, // Non-streaming for research loop
    };

    if (tools && tools.length > 0) {
      body.tools = tools;
      body.tool_choice = 'auto';
    }

    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: abortSignal,
    });

    if (!response.ok) {
      throw new Error(`LLM request failed: ${response.status} ${response.statusText}`);
    }

    const data = await response.json();
    const choice = data.choices?.[0];

    if (!choice) {
      throw new Error('No choices in LLM response');
    }

    const content = choice.message?.content || '';
    const toolCalls = choice.message?.tool_calls || [];
    const finishReason = choice.finish_reason || 'stop';

    return {
      content,
      toolCalls: toolCalls.map((tc: Record<string, unknown>) => ({
        id: tc.id,
        type: 'function',
        function: {
          name: (tc.function as Record<string, unknown>)?.name,
          arguments: (tc.function as Record<string, unknown>)?.arguments,
        },
      })),
      finishReason:
        finishReason === 'tool_calls'
          ? 'tool_calls'
          : finishReason === 'length'
          ? 'length'
          : 'stop',
    };
  };
}

// =============================================================================
// Debounced Persistence
// =============================================================================

/**
 * Create a debounced persistence function.
 */
function createDebouncedPersist(
  onPersist: (state: ResearchState) => Promise<void>,
  debounceMs: number
): {
  persist: (state: ResearchState) => void;
  flush: () => Promise<void>;
  cancel: () => void;
} {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  let pendingState: ResearchState | null = null;
  let persistPromise: Promise<void> | null = null;

  const persist = (state: ResearchState) => {
    pendingState = state;

    if (timeoutId) {
      clearTimeout(timeoutId);
    }

    timeoutId = setTimeout(async () => {
      timeoutId = null;
      if (pendingState) {
        const stateToSave = pendingState;
        pendingState = null;
        persistPromise = onPersist(stateToSave).catch((err) => {
          console.error('[useDeepResearch] Persistence error:', err);
        });
        await persistPromise;
        persistPromise = null;
      }
    }, debounceMs);
  };

  const flush = async () => {
    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
    if (pendingState) {
      const stateToSave = pendingState;
      pendingState = null;
      await onPersist(stateToSave);
    }
    if (persistPromise) {
      await persistPromise;
    }
  };

  const cancel = () => {
    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
    pendingState = null;
  };

  return { persist, flush, cancel };
}

// =============================================================================
// Hook Implementation
// =============================================================================

/**
 * Hook for managing deep research sessions.
 *
 * Provides:
 * - Automatic state persistence with debouncing
 * - Graceful stop via AbortController
 * - Integration with gglib's tool registry and LLM proxy
 */
export function useDeepResearch(
  options: UseDeepResearchOptions
): UseDeepResearchReturn {
  const {
    serverPort,
    conversationId,
    systemPrompt = '',
    maxSteps = DEFAULT_MAX_STEPS,
    onStateChange,
    onPersist,
    onError,
  } = options;

  // State
  const [state, setState] = useState<ResearchState | null>(null);
  const [isRunning, setIsRunning] = useState(false);

  // Refs for stable callbacks
  const abortControllerRef = useRef<AbortController | null>(null);
  const debouncedPersistRef = useRef<ReturnType<typeof createDebouncedPersist> | null>(
    null
  );

  // Create/update debounced persist when onPersist changes
  useEffect(() => {
    if (onPersist) {
      debouncedPersistRef.current = createDebouncedPersist(
        onPersist,
        PERSIST_DEBOUNCE_MS
      );
    } else {
      debouncedPersistRef.current = null;
    }

    return () => {
      debouncedPersistRef.current?.cancel();
    };
  }, [onPersist]);

  // Get available research tools
  const getResearchTools = useCallback((): ToolDefinition[] => {
    const registry = getToolRegistry();
    // Get all enabled tool definitions from the registry
    const allDefinitions = registry.getEnabledDefinitions();

    // Filter to research-relevant tools (search, web fetch, etc.)
    const researchToolNames = [
      'tavily_search',
      'tavily-search',
      'web_search',
      'search',
      'brave_search',
      'fetch',
      'web_fetch',
      'read_url',
      'scrape',
    ];

    // Filter and cast (the types are structurally compatible at runtime)
    const filtered = allDefinitions.filter((def) =>
      researchToolNames.some(
        (name) =>
          def.function.name.toLowerCase().includes(name) ||
          name.includes(def.function.name.toLowerCase())
      )
    );

    // Cast to our internal ToolDefinition type (compatible at runtime)
    return filtered as unknown as ToolDefinition[];
  }, []);

  // Handle state updates
  const handleStateUpdate = useCallback(
    (newState: ResearchState) => {
      setState(newState);
      onStateChange?.(newState);

      // Trigger debounced persistence
      debouncedPersistRef.current?.persist(newState);
    },
    [onStateChange]
  );

  // Start research
  const startResearch = useCallback(
    async (query: string, messageId: string) => {
      if (isRunning) {
        console.warn('[useDeepResearch] Already running, ignoring start request');
        return;
      }

      // Validate server
      if (!serverPort) {
        onError?.(new Error('No server selected'));
        return;
      }

      // Create abort controller
      const abortController = new AbortController();
      abortControllerRef.current = abortController;

      // Get tools
      const tools = getResearchTools();
      if (tools.length === 0) {
        onError?.(
          new Error(
            'No research tools available. Please enable a search tool (e.g., Tavily) in the MCP settings.'
          )
        );
        return;
      }

      // Create initial state
      const initialState = createInitialState(query, messageId, {
        conversationId,
        maxSteps,
      });

      setState(initialState);
      setIsRunning(true);

      // Model routing - use same server for now (can be extended later)
      const modelRouting: ModelRouting = createDefaultRouting(serverPort);

      try {
        const result = await runResearchLoop({
          query,
          messageId,
          conversationId,
          modelRouting,
          baseSystemPrompt: systemPrompt,
          tools,
          executeTool: createToolExecutor(),
          callLLM: createLLMCaller(),
          maxSteps,
          onStateUpdate: handleStateUpdate,
          onStatePersist: async (stateToSave) => {
            // Immediate persist for critical states
            await onPersist?.(stateToSave);
          },
          abortSignal: abortController.signal,
        });

        // Final state update
        setState(result.state);

        // Flush any pending persistence
        await debouncedPersistRef.current?.flush();

        if (!result.success && result.error) {
          onError?.(new Error(result.error));
        }
      } catch (error) {
        if (error instanceof Error && error.name === 'AbortError') {
          console.log('[useDeepResearch] Research aborted by user');
          // State is preserved, marked as incomplete
        } else {
          console.error('[useDeepResearch] Research error:', error);
          onError?.(error instanceof Error ? error : new Error(String(error)));
        }
      } finally {
        setIsRunning(false);
        abortControllerRef.current = null;

        // Final persistence flush
        await debouncedPersistRef.current?.flush();
      }
    },
    [
      isRunning,
      serverPort,
      conversationId,
      systemPrompt,
      maxSteps,
      getResearchTools,
      handleStateUpdate,
      onPersist,
      onError,
    ]
  );

  // Stop research (graceful)
  const stopResearch = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
  }, []);

  // Reset state
  const resetState = useCallback(() => {
    setState(null);
    setIsRunning(false);
    debouncedPersistRef.current?.cancel();
  }, []);

  return {
    state,
    isRunning,
    startResearch,
    stopResearch,
    resetState,
  };
}

export default useDeepResearch;
