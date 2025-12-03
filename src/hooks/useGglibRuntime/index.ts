/**
 * useGglibRuntime - Chat adapter for assistant-ui with SSE streaming and tool calling.
 *
 * This module provides:
 * - `useGglibRuntime` - Main hook for creating the chat runtime
 * - `fetchAvailableServers` - Utility to list available llama-server instances
 * - `parseSSEStream` - SSE stream parser (exported for testing)
 * - `createToolCallAccumulator` - Tool call accumulator factory (exported for testing)
 * - `createThinkingContentHandler` - Thinking content handler factory (exported for testing)
 *
 * @module useGglibRuntime
 */

// Main hook and utilities
export {
  useGglibRuntime,
  fetchAvailableServers,
  type ServerInfo,
  type GglibRuntimeOptions,
} from './useGglibRuntime';

// SSE parsing (exported for testing and potential reuse)
export {
  parseSSEStream,
  type StreamDelta,
  type ToolCallDelta,
  type ToolCallFunctionDelta,
} from './parseSSEStream';

// Tool call accumulation (exported for testing)
export {
  createToolCallAccumulator,
  type AccumulatedToolCall,
  type ToolCallAccumulator,
  type ToolCallAccumulatorState,
} from './accumulateToolCalls';

// Thinking content handling (exported for testing)
export {
  createThinkingContentHandler,
  type ThinkingContentHandler,
  type ThinkingState,
  type InlineThinkingState,
  type ThinkingDisplayState,
} from './thinkingContentHandler';
